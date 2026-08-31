use super::*;

fn temp_test_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ocg-browser-{label}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn tombstone(root: &Path, account_id: &str, nonce: u128) -> PathBuf {
    root.join(format!(
        "{PROFILE_TOMBSTONE_PREFIX}{account_id}-{nonce:032x}"
    ))
}

#[cfg(unix)]
fn symlink_directory(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_directory(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[cfg(unix)]
fn remove_directory_symlink(link: &Path) -> std::io::Result<()> {
    std::fs::remove_file(link)
}

#[cfg(windows)]
fn remove_directory_symlink(link: &Path) -> std::io::Result<()> {
    std::fs::remove_dir(link)
}

#[test]
fn account_profile_id_rejects_path_traversal() {
    for invalid in ["", ".", "..", "a/b", "a\\b", "C:evil", "two words"] {
        assert!(
            validate_account_id(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
    assert!(validate_account_id("98c28790-94fe-48d8_a").is_ok());
}

#[test]
fn profile_roots_honor_external_override_and_keep_legacy_root() {
    let data_dir = temp_test_root("profile-root-override");
    let external = data_dir.join("external-profiles");
    let roots = browser_profile_roots_with_override(
        &data_dir,
        Some(external.to_str().expect("test path should be Unicode")),
    )
    .unwrap();
    assert_eq!(roots, [external, data_dir.join("profiles")]);

    let legacy = data_dir.join("profiles");
    let roots = browser_profile_roots_with_override(
        &data_dir,
        Some(legacy.to_str().expect("test path should be Unicode")),
    )
    .unwrap();
    assert_eq!(roots, [legacy]);
    assert!(browser_profile_roots_with_override(&data_dir, Some("relative/path")).is_err());
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn browser_urls_require_safe_absolute_https() {
    assert!(validate_browser_url("https://accounts.google.com/signup").is_ok());
    assert!(validate_browser_url("http://accounts.google.com/signup").is_err());
    assert!(validate_browser_url("https://user:pass@opencode.ai/").is_err());
    assert!(validate_browser_url("javascript:alert(1)").is_err());
}

#[test]
fn remote_sessions_expire_on_idle_or_absolute_deadline() {
    // Advance a synthetic clock so the test does not depend on machine
    // uptime (subtracting a long duration from a fresh Windows Instant
    // can underflow immediately after boot).
    let now = Instant::now() + SESSION_MAX_LIFETIME + Duration::from_secs(60);
    let base = RemoteSession {
        account_id: "account-1".into(),
        binding: "admin".into(),
        worker_ws_url: "ws://browser:6080/websockify".into(),
        created_at: now - Duration::from_secs(60),
        last_active: now - Duration::from_secs(60),
        cancellation: tokio::sync::watch::channel(false).0,
    };
    assert!(!session_expired(&base, now));
    let idle = RemoteSession {
        last_active: now - SESSION_IDLE_TIMEOUT,
        ..base.clone()
    };
    assert!(session_expired(&idle, now));
    let absolute = RemoteSession {
        created_at: now - SESSION_MAX_LIFETIME,
        last_active: now,
        ..base
    };
    assert!(session_expired(&absolute, now));
}

#[test]
fn invalidating_remote_sessions_revokes_every_existing_view() {
    let runtime = BrowserRuntime::new();
    let now = Instant::now();
    let (cancellation, receiver) = tokio::sync::watch::channel(false);
    runtime.sessions.lock().insert(
        "old-token".into(),
        RemoteSession {
            account_id: "account-1".into(),
            binding: "admin".into(),
            worker_ws_url: "ws://browser:6080/websockify".into(),
            created_at: now,
            last_active: now,
            cancellation,
        },
    );
    runtime.invalidate_remote_sessions();
    assert!(!runtime.remote_session_active("old-token"));
    assert!(*receiver.borrow());
}

#[tokio::test]
async fn browser_operations_are_globally_serialized() {
    let runtime = BrowserRuntime::new();
    let operation = runtime.operation().await;
    assert!(runtime.operations.try_lock().is_err());
    drop(operation);
    assert!(runtime.operations.try_lock().is_ok());
}

#[test]
fn staged_profile_can_be_restored_and_purged() {
    let root = std::env::temp_dir().join(format!(
        "ocg-browser-profile-test-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let profile = root.join("browser-profiles").join("account-1");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("Cookies"), b"sensitive").unwrap();
    std::fs::write(profile.join("SingletonLock"), b"running").unwrap();
    assert!(
        StagedBrowserProfiles::stage(
            &root,
            "account-1",
            BrowserProfileOperationKind::DeleteAccount
        )
        .is_err()
    );
    std::fs::remove_file(profile.join("SingletonLock")).unwrap();
    let staged = StagedBrowserProfiles::stage(
        &root,
        "account-1",
        BrowserProfileOperationKind::DeleteAccount,
    )
    .unwrap();
    assert!(!profile.exists());
    staged.restore().unwrap();
    assert!(profile.join("Cookies").is_file());

    let staged = StagedBrowserProfiles::stage(
        &root,
        "account-1",
        BrowserProfileOperationKind::ResetProfile,
    )
    .unwrap();
    staged.purge().unwrap();
    assert!(!profile.exists());
    std::fs::remove_dir_all(root).unwrap();
}

/// Production reset path: stage(ResetProfile) then purge must clear both the
/// current and legacy roots for the target account, without touching siblings.
#[test]
fn reset_profile_stages_and_purges_new_and_legacy_roots_only() {
    let data_dir = temp_test_root("reset-new-and-legacy");
    let account_id = "account-1";
    let new_profile = data_dir.join("browser-profiles").join(account_id);
    let legacy_profile = data_dir.join("profiles").join(account_id);
    let other_profile = data_dir.join("browser-profiles").join("account-2");
    std::fs::create_dir_all(&new_profile).unwrap();
    std::fs::create_dir_all(&legacy_profile).unwrap();
    std::fs::create_dir_all(&other_profile).unwrap();
    std::fs::write(new_profile.join("Cookies"), b"new").unwrap();
    std::fs::write(legacy_profile.join("Cookies"), b"legacy").unwrap();
    std::fs::write(other_profile.join("Cookies"), b"sibling").unwrap();

    let staged = StagedBrowserProfiles::stage(
        &data_dir,
        account_id,
        BrowserProfileOperationKind::ResetProfile,
    )
    .expect("production reset stages current and legacy roots");
    assert!(!new_profile.exists());
    assert!(!legacy_profile.exists());
    assert!(other_profile.join("Cookies").is_file());
    staged
        .purge()
        .expect("production reset purges staged profiles");

    assert!(!new_profile.exists());
    assert!(!legacy_profile.exists());
    assert!(other_profile.join("Cookies").is_file());
    assert_eq!(
        std::fs::read(other_profile.join("Cookies")).unwrap(),
        b"sibling"
    );
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn staged_profile_rejects_non_directory_targets() {
    let root = std::env::temp_dir().join(format!(
        "ocg-browser-profile-file-test-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let profile = root.join("browser-profiles").join("account-1");
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    std::fs::write(&profile, b"not a profile directory").unwrap();
    assert!(
        StagedBrowserProfiles::stage(
            &root,
            "account-1",
            BrowserProfileOperationKind::DeleteAccount
        )
        .is_err()
    );
    assert!(profile.is_file());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reset_journal_purges_external_profile_even_when_current_root_changes() {
    let data_dir = temp_test_root("profile-journal-reset");
    let external_root_a = temp_test_root("profile-journal-root-a");
    let external_root_b = temp_test_root("profile-journal-root-b");
    let original = external_root_a.join("account-1");
    std::fs::create_dir_all(&original).unwrap();
    std::fs::write(original.join("Cookies"), b"old-cookie").unwrap();

    let staged = StagedBrowserProfiles::stage_paths(
        &data_dir,
        "account-1",
        BrowserProfileOperationKind::ResetProfile,
        vec![original.clone()],
    )
    .unwrap();
    let journal = staged.journal.as_ref().unwrap().clone();
    let journal_path = staged.journal_path.as_ref().unwrap().clone();
    let tombstone = journal.paths[0].tombstone.clone();
    assert!(journal.paths[0].original.is_absolute());
    assert!(tombstone.is_absolute());
    assert!(journal_path.starts_with(std::fs::canonicalize(&data_dir).unwrap()));
    drop(staged); // simulate a crash after the rename

    let report = recover_browser_profiles_with_roots(
        &data_dir,
        Ok(vec![external_root_b.clone()]),
        None,
        &mut |_| Ok(true),
    );

    assert_eq!(report.restored, 0);
    assert_eq!(report.purged, 1);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(!original.exists());
    assert!(!tombstone.exists());
    assert!(!journal_path.exists());
    std::fs::remove_dir_all(data_dir).unwrap();
    std::fs::remove_dir_all(external_root_a).unwrap();
    std::fs::remove_dir_all(external_root_b).unwrap();
}

#[test]
fn delete_journal_restores_before_commit_and_purges_after_commit() {
    for (account_exists, should_restore) in [(true, true), (false, false)] {
        let data_dir = temp_test_root(if account_exists {
            "profile-journal-delete-precommit"
        } else {
            "profile-journal-delete-committed"
        });
        let root = data_dir.join("external");
        let original = root.join("account-1");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::write(original.join("Cookies"), b"cookie").unwrap();
        let staged = StagedBrowserProfiles::stage_paths(
            &data_dir,
            "account-1",
            BrowserProfileOperationKind::DeleteAccount,
            vec![original.clone()],
        )
        .unwrap();
        let journal = staged.journal.as_ref().unwrap().clone();
        let journal_path = staged.journal_path.as_ref().unwrap().clone();
        let tombstone = journal.paths[0].tombstone.clone();
        drop(staged);

        let report =
            recover_browser_profiles_with_roots(&data_dir, Ok(Vec::new()), None, &mut |_| {
                Ok(account_exists)
            });

        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert_eq!(report.restored, usize::from(should_restore));
        assert_eq!(report.purged, usize::from(!should_restore));
        assert_eq!(original.is_dir(), should_restore);
        assert!(!tombstone.exists());
        assert!(!journal_path.exists());
        if should_restore {
            assert_eq!(std::fs::read(original.join("Cookies")).unwrap(), b"cookie");
        }
        std::fs::remove_dir_all(data_dir).unwrap();
    }
}

#[test]
fn journal_recovery_preserves_unsafe_profile_files() {
    let data_dir = temp_test_root("profile-journal-unsafe-file");
    let root = data_dir.join("external");
    let original = root.join("account-1");
    std::fs::create_dir_all(&original).unwrap();
    let staged = StagedBrowserProfiles::stage_paths(
        &data_dir,
        "account-1",
        BrowserProfileOperationKind::ResetProfile,
        vec![original.clone()],
    )
    .unwrap();
    let journal_path = staged.journal_path.as_ref().unwrap().clone();
    let tombstone = staged.journal.as_ref().unwrap().paths[0].tombstone.clone();
    drop(staged);
    std::fs::remove_dir_all(&tombstone).unwrap();
    std::fs::write(&tombstone, b"do not delete").unwrap();

    let report =
        recover_browser_profiles_with_roots(&data_dir, Ok(vec![root]), None, &mut |_| Ok(true));

    assert_eq!(report.restored, 0);
    assert_eq!(report.purged, 0);
    assert!(!report.issues.is_empty());
    assert_eq!(std::fs::read(&tombstone).unwrap(), b"do not delete");
    assert!(journal_path.is_file());
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn account_scoped_recovery_validates_other_journals_before_legacy_scan() {
    let data_dir = temp_test_root("profile-journal-validation-order");
    let root = data_dir.join("browser-profiles");
    std::fs::create_dir_all(&root).unwrap();
    let legacy_tombstone = tombstone(&root, "target-account", 1);
    std::fs::create_dir(&legacy_tombstone).unwrap();
    std::fs::write(legacy_tombstone.join("Cookies"), b"keep-until-safe").unwrap();

    let invalid_journal = BrowserProfileOperationJournal {
        version: PROFILE_OPERATION_JOURNAL_VERSION + 1,
        operation_id: uuid::Uuid::new_v4().simple().to_string(),
        account_id: "other-account".into(),
        kind: BrowserProfileOperationKind::DeleteAccount,
        paths: vec![BrowserProfileOperationPath {
            original: std::fs::canonicalize(&root).unwrap().join("other-account"),
            tombstone: tombstone(&std::fs::canonicalize(&root).unwrap(), "other-account", 2),
        }],
    };
    let journal_path = persist_profile_operation_journal(&data_dir, &invalid_journal).unwrap();

    let report = recover_browser_profiles_with_roots(
        &data_dir,
        Ok(vec![root.clone()]),
        Some("target-account"),
        &mut |_| Ok(true),
    );

    assert_eq!(report.restored, 0);
    assert_eq!(report.purged, 0);
    assert!(!report.issues.is_empty());
    assert!(journal_path.is_file());
    assert!(legacy_tombstone.is_dir());
    assert!(!root.join("target-account").exists());
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn directory_sync_wrapper_accepts_real_profile_directories() {
    let data_dir = temp_test_root("profile-directory-sync");
    let root = data_dir.join("browser-profiles");
    std::fs::create_dir(&root).unwrap();

    sync_directory(&data_dir, "test data directory").unwrap();
    sync_directory(&root, "test browser profile root").unwrap();

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[cfg(any(unix, windows))]
#[test]
fn journal_recovery_never_follows_profile_symlinks() {
    let data_dir = temp_test_root("profile-journal-unsafe-symlink");
    let outside = temp_test_root("profile-journal-unsafe-symlink-target");
    let sentinel = outside.join("sentinel");
    std::fs::write(&sentinel, b"keep").unwrap();
    let root = data_dir.join("external");
    let original = root.join("account-1");
    std::fs::create_dir_all(&original).unwrap();
    let staged = StagedBrowserProfiles::stage_paths(
        &data_dir,
        "account-1",
        BrowserProfileOperationKind::ResetProfile,
        vec![original],
    )
    .unwrap();
    let journal_path = staged.journal_path.as_ref().unwrap().clone();
    let tombstone = staged.journal.as_ref().unwrap().paths[0].tombstone.clone();
    drop(staged);
    std::fs::remove_dir_all(&tombstone).unwrap();
    if let Err(error) = symlink_directory(&outside, &tombstone) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || (cfg!(windows) && error.raw_os_error() == Some(1314))
        {
            std::fs::remove_dir_all(data_dir).unwrap();
            std::fs::remove_dir_all(outside).unwrap();
            return;
        }
        panic!("failed to create directory symlink: {error}");
    }

    let report =
        recover_browser_profiles_with_roots(&data_dir, Ok(vec![root]), None, &mut |_| Ok(true));

    assert_eq!(report.restored, 0);
    assert_eq!(report.purged, 0);
    assert!(!report.issues.is_empty());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
    assert!(journal_path.is_file());
    remove_directory_symlink(&tombstone).unwrap();
    std::fs::remove_dir_all(data_dir).unwrap();
    std::fs::remove_dir_all(outside).unwrap();
}

#[test]
fn recovery_restores_one_tombstone_per_profile_root() {
    let data_dir = temp_test_root("profile-recovery-restore");
    let roots = [data_dir.join("browser-profiles"), data_dir.join("profiles")];
    for (index, root) in roots.iter().enumerate() {
        std::fs::create_dir_all(root).unwrap();
        let staged = tombstone(root, "account-1", index as u128 + 1);
        std::fs::create_dir(&staged).unwrap();
        std::fs::write(staged.join("Cookies"), format!("root-{index}")).unwrap();
    }

    let mut account_reads = 0;
    let report = recover_staged_browser_profiles_in_roots(roots.to_vec(), None, |account_id| {
        account_reads += 1;
        assert_eq!(account_id, "account-1");
        Ok(true)
    });

    assert_eq!(report.restored, 2);
    assert_eq!(report.purged, 0);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(account_reads, 1);
    for (index, root) in roots.iter().enumerate() {
        assert_eq!(
            std::fs::read_to_string(root.join("account-1").join("Cookies")).unwrap(),
            format!("root-{index}")
        );
    }
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn recovery_purges_committed_and_stale_tombstones() {
    let data_dir = temp_test_root("profile-recovery-purge");
    let deleted_root = data_dir.join("browser-profiles");
    let existing_root = data_dir.join("profiles");
    std::fs::create_dir_all(&deleted_root).unwrap();
    std::fs::create_dir_all(existing_root.join("existing-account")).unwrap();

    let deleted = [
        tombstone(&deleted_root, "deleted-account", 1),
        tombstone(&deleted_root, "deleted-account", 2),
    ];
    let stale = [
        tombstone(&existing_root, "existing-account", 3),
        tombstone(&existing_root, "existing-account", 4),
    ];
    for path in deleted.iter().chain(&stale) {
        std::fs::create_dir(path).unwrap();
        std::fs::write(path.join("Cookies"), b"sensitive").unwrap();
    }

    let report = recover_staged_browser_profiles_in_roots(
        vec![deleted_root, existing_root.clone()],
        None,
        |account_id| Ok(account_id == "existing-account"),
    );

    assert_eq!(report.restored, 0);
    assert_eq!(report.purged, 4);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(existing_root.join("existing-account").is_dir());
    assert!(deleted.iter().chain(&stale).all(|path| !path.exists()));
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn recovery_preserves_ambiguous_or_unsafe_tombstones() {
    let data_dir = temp_test_root("profile-recovery-ambiguous");
    let root = data_dir.join("browser-profiles");
    std::fs::create_dir_all(&root).unwrap();
    let ambiguous = [
        tombstone(&root, "ambiguous-account", 1),
        tombstone(&root, "ambiguous-account", 2),
    ];
    for path in &ambiguous {
        std::fs::create_dir(path).unwrap();
    }
    let unsafe_file = tombstone(&root, "unsafe-account", 3);
    std::fs::write(&unsafe_file, b"not a directory").unwrap();
    let malformed = root.join(".ocg-profile-delete-ignored-account-not-a-uuid");
    std::fs::create_dir(&malformed).unwrap();

    let report = recover_staged_browser_profiles_in_roots(vec![root], None, |_| Ok(true));

    assert_eq!(report.restored, 0);
    assert_eq!(report.purged, 0);
    assert_eq!(report.issues.len(), 2, "{:?}", report.issues);
    assert!(ambiguous.iter().all(|path| path.is_dir()));
    assert!(unsafe_file.is_file());
    assert!(malformed.is_dir());
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn target_recovery_makes_a_failed_delete_cleanup_retriable() {
    let data_dir = temp_test_root("profile-recovery-retry");
    let root = data_dir.join("profiles");
    std::fs::create_dir_all(&root).unwrap();
    let staged = tombstone(&root, "deleted-account", 1);
    std::fs::create_dir(&staged).unwrap();
    std::fs::write(staged.join("Cookies"), b"sensitive").unwrap();

    let report =
        recover_staged_browser_profiles_for_account(&data_dir, "deleted-account", false).unwrap();

    assert_eq!(report.purged, 1);
    assert!(!staged.exists());
    std::fs::remove_dir_all(data_dir).unwrap();
}

#[cfg(any(unix, windows))]
#[test]
fn recovery_and_staging_never_follow_directory_symlinks() {
    let data_dir = temp_test_root("profile-recovery-symlink");
    let outside = temp_test_root("profile-recovery-symlink-target");
    let sentinel = outside.join("sentinel");
    std::fs::write(&sentinel, b"keep").unwrap();

    let root = data_dir.join("browser-profiles");
    std::fs::create_dir(&root).unwrap();
    let tombstone_link = tombstone(&root, "deleted-account", 1);
    if let Err(error) = symlink_directory(&outside, &tombstone_link) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || (cfg!(windows) && error.raw_os_error() == Some(1314))
        {
            std::fs::remove_dir_all(data_dir).unwrap();
            std::fs::remove_dir_all(outside).unwrap();
            return;
        }
        panic!("failed to create directory symlink: {error}");
    }

    let report = recover_staged_browser_profiles_in_roots(vec![root], None, |_| Ok(false));
    assert_eq!(report.purged, 0);
    assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
    assert!(
        std::fs::symlink_metadata(&tombstone_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
    remove_directory_symlink(&tombstone_link).unwrap();

    let legacy_root = data_dir.join("profiles");
    symlink_directory(&outside, &legacy_root).unwrap();
    std::fs::create_dir(outside.join("account-1")).unwrap();
    assert!(
        StagedBrowserProfiles::stage(
            &data_dir,
            "account-1",
            BrowserProfileOperationKind::DeleteAccount
        )
        .is_err()
    );
    assert!(outside.join("account-1").is_dir());
    remove_directory_symlink(&legacy_root).unwrap();

    std::fs::remove_dir_all(data_dir).unwrap();
    std::fs::remove_dir_all(outside).unwrap();
}
