use crate::{
    BookOrbit, HttpFailure, Result, checked, is_html, is_unauthorized, raw_bounded, valid_token,
};
use reqwest::{
    StatusCode,
    blocking::Response,
    header::{ACCEPT, CONTENT_TYPE},
};
use serde::Deserialize;
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

/// All workers using clones of a session share the latest credential pair.
#[derive(Clone)]
pub struct Session {
    api: BookOrbit,
    credentials: Arc<Mutex<Option<Arc<Tokens>>>>,
}

struct Tokens {
    access: String,
    refresh: Option<String>,
}

fn valid_refresh(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn expired() -> HttpFailure {
    HttpFailure {
        status: StatusCode::UNAUTHORIZED,
        description: Some("Войдите в BookOrbit снова."),
    }
}

impl BookOrbit {
    pub fn sign_in(&self, username: &str, password: &str) -> Result<Session> {
        if username.trim().is_empty() || password.is_empty() {
            return Err("Введите имя пользователя и пароль BookOrbit.".into());
        }
        let body = serde_json::json!({"username":username,"password":password,"clientKind":"native","deviceLabel":"botui"});
        let tokens = parse_login(self.auth_post("auth/login", &body, Duration::from_secs(15))?)?;
        Ok(Session {
            api: self.clone(),
            credentials: Arc::new(Mutex::new(Some(Arc::new(tokens)))),
        })
    }

    fn auth_post(
        &self,
        path: &str,
        body: &serde_json::Value,
        timeout: Duration,
    ) -> Result<Response> {
        Ok(self
            .client
            .post(self.root.join(path)?)
            .timeout(timeout)
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(body)?)
            .send()
            .map_err(|e| e.without_url())?)
    }

    fn rotate(&self, refresh: &str) -> Result<Tokens> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Rotated {
            access_token: String,
            refresh_token: String,
        }

        let reply = checked(self.auth_post(
            "auth/refresh",
            &serde_json::json!({"refreshToken":refresh}),
            Duration::from_secs(8),
        )?)?;
        let bytes = raw_bounded(reply, 1024 * 1024)?;
        let new = serde_json::from_slice::<Rotated>(&bytes)
            .ok()
            .filter(|p| valid_token(&p.access_token) && valid_refresh(&p.refresh_token))
            .ok_or_else(expired)?;
        Ok(Tokens {
            access: new.access_token,
            refresh: Some(new.refresh_token),
        })
    }
}

impl Session {
    fn lock(&self) -> Result<MutexGuard<'_, Option<Arc<Tokens>>>> {
        self.credentials
            .lock()
            .map_err(|_| "Сессия BookOrbit недоступна.".into())
    }

    /// Run an operation with the current credentials. Only a 401 is retried, once.
    pub fn with_token<T>(
        &self,
        mut operation: impl FnMut(&BookOrbit, &str) -> Result<T>,
    ) -> Result<T> {
        let initial = self.lock()?.clone().ok_or_else(expired)?;
        match operation(&self.api, &initial.access) {
            Err(err) if is_unauthorized(err.as_ref()) => {}
            result => return result,
        }
        let fresh = self.after_unauthorized(&initial)?;
        let result = operation(&self.api, &fresh.access);
        if result.as_ref().is_err_and(|e| is_unauthorized(e.as_ref())) {
            let mut state = self.lock()?;
            if state
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &fresh))
            {
                *state = None;
            }
        }
        result
    }

    fn after_unauthorized(&self, used: &Arc<Tokens>) -> Result<Arc<Tokens>> {
        // Refresh tokens are single-use. Serialize rotation, then reuse its result even
        // when a server issues an identical JWT string for two concurrent refreshes.
        let mut state = self.lock()?;
        let current = state.as_ref().ok_or_else(expired)?;
        if !Arc::ptr_eq(current, used) {
            return Ok(Arc::clone(current));
        }
        let Some(refresh) = current.refresh.as_deref() else {
            *state = None;
            return Err(expired().into());
        };
        match self.api.rotate(refresh) {
            Ok(tokens) => {
                let next = Arc::new(tokens);
                *state = Some(Arc::clone(&next));
                Ok(next)
            }
            Err(err) => {
                if is_unauthorized(err.as_ref()) {
                    *state = None;
                }
                Err(err)
            }
        }
    }

    /// Clear credentials for all clones and revoke the current native session.
    /// A connection failure never restores local credentials. Drop performs no I/O.
    pub fn logout(&self) -> Result<()> {
        let tokens = self.lock()?.take();
        if let Some(refresh) = tokens.as_ref().and_then(|t| t.refresh.as_ref()) {
            checked(self.api.auth_post(
                "auth/logout",
                &serde_json::json!({"refreshToken":refresh}),
                Duration::from_secs(3),
            )?)?;
        }
        Ok(())
    }
}

fn parse_login(response: Response) -> Result<Tokens> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reply {
        access_token: Option<String>,
        refresh_token: Option<String>,
        error_code: Option<String>,
    }

    let status = response.status();
    let html = is_html(&response);
    let reply = serde_json::from_slice::<Reply>(&raw_bounded(response, 1024 * 1024)?).ok();
    if !status.is_success() {
        let code = reply.as_ref().and_then(|v| v.error_code.as_deref());
        let description = match code {
            Some("password_auth_disabled") => {
                Some("Вход по паролю отключён. Для OIDC требуется вход через браузер.")
            }
            Some("account_locked") => {
                Some("Учётная запись временно заблокирована после неудачных попыток входа.")
            }
            _ if status.is_redirection() || html => {
                Some("Сервер запросил вход через браузер (OIDC или внешний провайдер).")
            }
            _ if status == StatusCode::UNAUTHORIZED => Some("Проверьте имя пользователя и пароль."),
            _ => None,
        };
        return Err(HttpFailure {
            status,
            description,
        }
        .into());
    }
    if html {
        return Err(
            "Сервер вернул страницу входа. Для OIDC или внешней проверки требуется браузер.".into(),
        );
    }
    let missing = "Сервер не выдал токен BookOrbit. Для OIDC или дополнительной проверки входа требуется браузер.";
    let reply = reply.ok_or(missing)?;
    Ok(Tokens {
        access: reply
            .access_token
            .filter(|v| valid_token(v))
            .ok_or(missing)?,
        refresh: reply.refresh_token.filter(|v| valid_refresh(v)),
    })
}
