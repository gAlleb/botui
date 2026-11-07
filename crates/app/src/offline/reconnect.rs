//! Retry queued positions in the running app. Interactive sign-in is still possible
//! when the user hasn't configured unattended credentials.
use super::{account, has_pending, sync};
use crate::settings::Settings;
use botui_bookorbit::{BookOrbit, Session};
use std::sync::{Arc, Mutex, OnceLock, atomic::AtomicBool, mpsc};
use std::time::Duration;

enum Event {
    Wake,
    Connected(String, Arc<Session>),
    SignedOut,
}

static WORK: OnceLock<mpsc::Sender<Event>> = OnceLock::new();
static NOTICE: Mutex<Option<String>> = Mutex::new(None);

pub(crate) fn start() {
    WORK.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || run(receiver));
        sender
    });
}

pub(crate) fn wake() {
    send(Event::Wake);
}
pub(crate) fn connected(account: String, session: Arc<Session>) {
    send(Event::Connected(account, session));
}
pub(crate) fn signed_out() {
    send(Event::SignedOut);
}
fn send(event: Event) {
    if let Some(tx) = WORK.get() {
        let _ = tx.send(event);
    }
}

pub(crate) fn notice() -> Option<String> {
    NOTICE.lock().ok()?.take()
}

fn run(rx: mpsc::Receiver<Event>) {
    let mut live = None;
    let mut signed_out = false;
    let mut delay = Duration::from_secs(3);
    loop {
        let cancel = AtomicBool::new(false);
        let attempt = if signed_out {
            Ok(None)
        } else {
            try_sync(live.as_ref(), &cancel)
        };
        let event = match attempt {
            Err(e)
                if !botui_bookorbit::is_unauthorized(e.as_ref())
                    && !botui_bookorbit::is_forbidden(e.as_ref()) =>
            {
                match rx.recv_timeout(delay) {
                    Ok(event) => event,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        delay = (delay * 2).min(Duration::from_secs(60));
                        continue;
                    }
                    Err(_) => break,
                }
            }
            result => {
                let message = match result {
                    Ok(message) => message,
                    Err(_) => {
                        Some("Sign in to BookOrbit to sync your saved reading position.".into())
                    }
                };
                if message.is_some()
                    && let Ok(mut slot) = NOTICE.lock()
                {
                    *slot = message;
                }
                let Ok(event) = rx.recv() else { break };
                event
            }
        };
        delay = Duration::from_secs(3);
        match event {
            Event::Connected(account, session) => {
                live = Some((account, session));
                signed_out = false;
            }
            Event::SignedOut => {
                live = None;
                signed_out = true;
            }
            Event::Wake => (),
        }
    }
}

fn try_sync(
    live: Option<&(String, Arc<Session>)>,
    cancel: &AtomicBool,
) -> botui_bookorbit::Result<Option<String>> {
    if let Some((account, session)) = live {
        return sync(session, account, cancel);
    }
    let connection = Settings::load()?.bookorbit;
    if !connection.has_credentials() {
        return Ok(None);
    }
    let id = account(&connection.server, &connection.username);
    if !has_pending(&id)? {
        return Ok(None);
    }
    let client = BookOrbit::new(&connection.server)?;
    let session = client.sign_in(&connection.username, &connection.resolve(cancel)?)?;
    let result = sync(&session, &id, cancel);
    let _ = session.logout();
    result
}
