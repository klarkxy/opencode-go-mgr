use crate::state::AppState;
use chrono::Utc;
use ocg_core::models::{Account, AccountInput, AccountUpdate};
use tauri::State;

#[tauri::command]
pub fn get_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, String> {
    state.core.db.lock().list_accounts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_account(state: State<'_, AppState>, input: AccountInput) -> Result<Account, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    let account = Account {
        id: id.clone(),
        name: input.name,
        key_cipher: state.core.encrypt_key(&input.key).map_err(|e| e.to_string())?,
        enabled: true,
        referral_code: input.referral_code,
        recharge_date: input.recharge_date,
        cooldown_until: None,
        last_error: None,
        created_at: now,
        updated_at: now,
    };
    let db = state.core.db.lock();
    db.create_account(&account).map_err(|e| e.to_string())?;
    let _ = db.log_gateway("info", "account", &format!("created account {}", account.name));
    drop(db);
    crate::commands::sync::push_one(state.inner().clone(), Some(account.clone()), crate::commands::sync::PushOp::Create);
    Ok(account)
}

#[tauri::command]
pub fn update_account(
    state: State<'_, AppState>,
    id: String,
    update: AccountUpdate,
) -> Result<Account, String> {
    let key_cipher = update
        .key
        .as_ref()
        .filter(|k| !k.is_empty())  // treat empty key string as "no update"
        .map(|k| state.core.encrypt_key(k))
        .transpose()
        .map_err(|e| e.to_string())?;
    {
        let db = state.core.db.lock();
        db.update_account(&id, &update, key_cipher.as_deref())
            .map_err(|e| e.to_string())?;
    }
    let db = state.core.db.lock();
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    let _ = db.log_gateway("info", "account", &format!("updated account {}", account.name));
    drop(db);
    crate::commands::sync::push_one(state.inner().clone(), Some(account.clone()), crate::commands::sync::PushOp::Update);
    Ok(account)
}

#[tauri::command]
pub fn delete_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let mut db = state.core.db.lock();
        if let Some(account) = db.get_account(&id).map_err(|e| e.to_string())? {
            db.delete_account(&id).map_err(|e| e.to_string())?;
            let _ = db.log_gateway("info", "account", &format!("deleted account {}", account.name));
            drop(db);
            crate::commands::sync::push_one(state.inner().clone(), Some(account), crate::commands::sync::PushOp::Delete);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_account(state: State<'_, AppState>, id: String) -> Result<Account, String> {
    let account = {
        let db = state.core.db.lock();
        db.get_account(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "account not found".to_string())?
    };
    let update = AccountUpdate {
        name: None,
        key: None,
        enabled: Some(!account.enabled),
        referral_code: None,
        recharge_date: None,
    };
    {
        let db = state.core.db.lock();
        db.update_account(&id, &update, None)
            .map_err(|e| e.to_string())?;
    }
    let db = state.core.db.lock();
    let account = db.get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found after toggle".to_string())?;
    drop(db);
    crate::commands::sync::push_one(state.inner().clone(), Some(account.clone()), crate::commands::sync::PushOp::Update);
    Ok(account)
}

#[tauri::command]
pub fn test_account(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let db = state.core.db.lock();
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    let key = state.core.decrypt_key(&account.key_cipher).map_err(|e| e.to_string())?;
    let masked = if key.len() > 8 && key.is_char_boundary(4) && key.is_char_boundary(key.len() - 4) {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "***".to_string()
    };
    Ok(format!("account {} key looks valid ({})", account.name, masked))
}

#[tauri::command]
pub fn get_account_usage(
    state: State<'_, AppState>,
    id: String,
) -> Result<ocg_core::models::UsageWindow, String> {
    state.core.db.lock().account_usage(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reset_account_cooldown(state: State<'_, AppState>, id: String) -> Result<Account, String> {
    {
        let db = state.core.db.lock();
        db.clear_account_cooldown(&id).map_err(|e| e.to_string())?;
    }
    let db = state.core.db.lock();
    let account = db
        .get_account(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "account not found".to_string())?;
    let _ = db.log_gateway("info", "account", &format!("reset cooldown for {}", account.name));
    drop(db);
    crate::commands::sync::push_one(state.inner().clone(), Some(account.clone()), crate::commands::sync::PushOp::Update);
    Ok(account)
}
