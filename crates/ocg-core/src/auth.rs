use crate::db::Database;
use anyhow::{Result, anyhow, bail};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use serde::{Deserialize, Serialize};

const ADMIN_SETTING: &str = "dashboard_admin";
pub const ADMIN_USERNAME_ENV: &str = "OCG_ADMIN_USERNAME";
pub const ADMIN_PASSWORD_ENV: &str = "OCG_ADMIN_PASSWORD";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DashboardAdmin {
    pub username: String,
    pub password_hash: String,
}

pub fn bootstrap_admin_from_env(db: &Database) -> Result<()> {
    if load_admin(db)?.is_some() {
        return Ok(());
    }

    let username = std::env::var_os(ADMIN_USERNAME_ENV);
    let password = std::env::var_os(ADMIN_PASSWORD_ENV);
    let (username, password) = match (username, password) {
        (None, None) => return Ok(()),
        (Some(username), Some(password)) => (
            username
                .into_string()
                .map_err(|_| anyhow!("{ADMIN_USERNAME_ENV} must be valid UTF-8"))?,
            password
                .into_string()
                .map_err(|_| anyhow!("{ADMIN_PASSWORD_ENV} must be valid UTF-8"))?,
        ),
        _ => bail!("{ADMIN_USERNAME_ENV} and {ADMIN_PASSWORD_ENV} must be set together"),
    };

    let admin = build_admin(&username, &password)?;
    save_admin(db, &admin)
}

pub fn load_admin(db: &Database) -> Result<Option<DashboardAdmin>> {
    db.get_setting(ADMIN_SETTING)?
        .map(|value| serde_json::from_str(&value).map_err(Into::into))
        .transpose()
}

pub fn build_admin(username: &str, password: &str) -> Result<DashboardAdmin> {
    let username = username.trim();
    let username_len = username.chars().count();
    if !(1..=64).contains(&username_len) {
        bail!("username must contain 1 to 64 characters");
    }
    let password_len = password.chars().count();
    if !(8..=256).contains(&password_len) {
        bail!("password must contain 8 to 256 characters");
    }

    let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes())
        .map_err(|e| anyhow!("failed to generate password salt: {e}"))?;
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("failed to hash password: {e}"))?
        .to_string();
    Ok(DashboardAdmin {
        username: username.to_string(),
        password_hash,
    })
}

pub fn save_admin(db: &Database, admin: &DashboardAdmin) -> Result<()> {
    db.set_setting(ADMIN_SETTING, &serde_json::to_string(admin)?)
}

pub fn verify_admin(admin: &DashboardAdmin, username: &str, password: &str) -> bool {
    let password_ok = PasswordHash::new(&admin.password_hash)
        .ok()
        .and_then(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .ok()
        })
        .is_some();
    password_ok && username.trim() == admin.username
}

#[cfg(test)]
mod tests {
    use super::{build_admin, verify_admin};

    #[test]
    fn password_hash_is_salted_and_verifies() {
        let first = build_admin(" admin ", "correct horse battery staple").unwrap();
        let second = build_admin("admin", "correct horse battery staple").unwrap();

        assert_eq!(first.username, "admin");
        assert_ne!(first.password_hash, second.password_hash);
        assert!(verify_admin(
            &first,
            "admin",
            "correct horse battery staple"
        ));
        assert!(!verify_admin(&first, "admin", "wrong password"));
    }

    #[test]
    fn rejects_incomplete_credentials() {
        assert!(build_admin("", "password123").is_err());
        assert!(build_admin("admin", "short").is_err());
    }
}
