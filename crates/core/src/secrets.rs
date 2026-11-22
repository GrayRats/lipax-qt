//! API keys in the user's wallet (the freedesktop Secret Service: KWallet via `ksecretd`, GNOME Keyring, KeePassXC).
//!
//! Opt-in (`general.keys_in_wallet`). The wallet is only ever asked to do what needs no user dialog: a locked
//! collection is reported as unavailable instead of opening an unlock prompt in the middle of a game, and every
//! call has a time limit. The caller keeps the plain `config.toml` value whenever the wallet cannot be used, so a
//! key is never lost because the wallet was missing.
//!
//! The session uses the `plain` algorithm: the value travels over the user's private session bus, as it does for
//! most Secret Service clients that do not link a crypto library. No new dependency is needed.

use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, Value};
use zbus::Connection;

const SERVICE: &str = "org.freedesktop.secrets";
const SERVICE_PATH: &str = "/org/freedesktop/secrets";
const COLLECTION_ALIAS: &str = "default";
const NO_PROMPT: &str = "/";
const APPLICATION: &str = "lipax";
const TIMEOUT: Duration = Duration::from_secs(4);

/// A place that keeps named secrets. The settings code talks to this, so it can be tested without a wallet.
pub trait SecretStore {
    /// `Ok(None)`: no such secret. `Err`: the store cannot be used right now.
    fn get(&self, name: &str) -> Result<Option<String>, String>;
    fn set(&self, name: &str, value: &str) -> Result<(), String>;
    fn remove(&self, name: &str) -> Result<(), String>;
}

/// The Secret Service of the session bus, called from synchronous code.
///
/// Settings are saved at every change, so two things keep that cheap: what the wallet is known to hold is
/// remembered (an unchanged value is not written again), and after a failure the wallet is not tried again for
/// [`RETRY_AFTER`], so a wallet that hangs costs one time-out and not one per save.
pub struct Wallet;

const RETRY_AFTER: Duration = Duration::from_secs(60);

static KNOWN: std::sync::Mutex<Option<HashMap<String, String>>> = std::sync::Mutex::new(None);
static FAILED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

fn known<T>(f: impl FnOnce(&mut HashMap<String, String>) -> T) -> T {
    f(KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new))
}

/// Applies the back-off and records a failure. `Ok(None)` from the wallet is a normal answer, not a failure.
fn guarded<T: Send + 'static>(future: impl std::future::Future<Output = Result<T, String>> + Send + 'static) -> Result<T, String> {
    let mut failed = FAILED_AT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = *failed && at.elapsed() < RETRY_AFTER {
        return Err("кошелёк недавно был недоступен".into());
    }
    let result = run(future);
    *failed = result.is_err().then(std::time::Instant::now);
    result
}

impl Wallet {
    /// Whether the wallet can be used now (for the Status tab).
    pub fn check() -> Result<(), String> { guarded(async { Session::open().await.map(|_| ()) }) }
}

impl SecretStore for Wallet {
    fn get(&self, name: &str) -> Result<Option<String>, String> {
        let value = guarded(get(name.to_owned()))?;
        known(|k| k.insert(name.to_owned(), value.clone().unwrap_or_default()));
        Ok(value)
    }
    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        if known(|k| k.get(name).is_some_and(|v| v == value)) { return Ok(()); }
        guarded(set(name.to_owned(), value.to_owned()))?;
        known(|k| k.insert(name.to_owned(), value.to_owned()));
        Ok(())
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        if known(|k| k.get(name).is_some_and(String::is_empty)) { return Ok(()); }
        guarded(remove(name.to_owned()))?;
        known(|k| k.insert(name.to_owned(), String::new()));
        Ok(())
    }
}

/// Runs the future on a thread of its own with a small runtime: callers are the GUI thread and async tasks alike,
/// and neither may be blocked for longer than [`TIMEOUT`].
fn run<T: Send + 'static>(future: impl std::future::Future<Output = Result<T, String>> + Send + 'static) -> Result<T, String> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        runtime.block_on(async { tokio::time::timeout(TIMEOUT, future).await.map_err(|_| "кошелёк не ответил вовремя".to_owned())? })
    })
    .join()
    .map_err(|_| "сбой потока кошелька".to_owned())?
}

fn error(e: impl std::fmt::Display) -> String { e.to_string() }

fn attributes(name: &str) -> HashMap<String, String> {
    HashMap::from([("application".to_owned(), APPLICATION.to_owned()), ("key".to_owned(), name.to_owned())])
}

type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

/// A plain session and the unlocked default collection.
struct Session {
    connection: Connection,
    session: OwnedObjectPath,
    collection: OwnedObjectPath,
}

impl Session {
    async fn open() -> Result<Self, String> {
        let connection = Connection::session().await.map_err(error)?;
        let reply = connection
            .call_method(Some(SERVICE), SERVICE_PATH, Some("org.freedesktop.Secret.Service"), "OpenSession", &("plain", Value::from("")))
            .await
            .map_err(|e| format!("служба секретов недоступна: {e}"))?;
        let (_output, session): (zbus::zvariant::OwnedValue, OwnedObjectPath) = reply.body().deserialize().map_err(error)?;
        let reply = connection
            .call_method(Some(SERVICE), SERVICE_PATH, Some("org.freedesktop.Secret.Service"), "ReadAlias", &(COLLECTION_ALIAS,))
            .await
            .map_err(error)?;
        let collection: OwnedObjectPath = reply.body().deserialize().map_err(error)?;
        if collection.as_str() == NO_PROMPT {
            return Err("в службе секретов нет коллекции по умолчанию".into());
        }
        let this = Self { connection, session, collection };
        this.ensure_unlocked().await?;
        Ok(this)
    }

    async fn ensure_unlocked(&self) -> Result<(), String> {
        let locked: bool = self.property(self.collection.as_str(), "org.freedesktop.Secret.Collection", "Locked").await?;
        if !locked {
            return Ok(());
        }
        let reply = self
            .connection
            .call_method(Some(SERVICE), SERVICE_PATH, Some("org.freedesktop.Secret.Service"), "Unlock", &(vec![self.collection.clone()],))
            .await
            .map_err(error)?;
        let (_unlocked, _prompt): (Vec<OwnedObjectPath>, OwnedObjectPath) = reply.body().deserialize().map_err(error)?;
        // The prompt of a locked wallet is never started (that would be a dialog in front of the game): the wallet counts as unavailable.
        let locked: bool = self.property(self.collection.as_str(), "org.freedesktop.Secret.Collection", "Locked").await?;
        if locked { Err("кошелёк заблокирован".into()) } else { Ok(()) }
    }

    async fn property<T: TryFrom<zbus::zvariant::OwnedValue>>(&self, path: &str, interface: &str, name: &str) -> Result<T, String>
    where
        T::Error: Into<zbus::zvariant::Error>,
    {
        let reply = self
            .connection
            .call_method(Some(SERVICE), path, Some("org.freedesktop.DBus.Properties"), "Get", &(interface, name))
            .await
            .map_err(error)?;
        let value: zbus::zvariant::OwnedValue = reply.body().deserialize().map_err(error)?;
        T::try_from(value).map_err(|e| error(e.into()))
    }

    /// The items of the application with this name (normally one).
    async fn find(&self, name: &str) -> Result<Vec<OwnedObjectPath>, String> {
        let reply = self
            .connection
            .call_method(Some(SERVICE), self.collection.as_str(), Some("org.freedesktop.Secret.Collection"), "SearchItems", &(attributes(name),))
            .await
            .map_err(error)?;
        reply.body().deserialize().map_err(error)
    }
}

async fn get(name: String) -> Result<Option<String>, String> {
    let session = Session::open().await?;
    let Some(item) = session.find(&name).await?.into_iter().next() else { return Ok(None) };
    let reply = session
        .connection
        .call_method(Some(SERVICE), item.as_str(), Some("org.freedesktop.Secret.Item"), "GetSecret", &(&session.session,))
        .await
        .map_err(error)?;
    let (_session, _parameters, value, _content_type): Secret = reply.body().deserialize().map_err(error)?;
    String::from_utf8(value).map(Some).map_err(error)
}

async fn set(name: String, value: String) -> Result<(), String> {
    let session = Session::open().await?;
    let label = format!("LipaX: {name}");
    let properties: HashMap<&str, Value<'_>> = HashMap::from([
        ("org.freedesktop.Secret.Item.Label", Value::from(label)),
        ("org.freedesktop.Secret.Item.Attributes", Value::from(attributes(&name))),
    ]);
    let secret: Secret = (session.session.clone(), Vec::new(), value.into_bytes(), "text/plain".to_owned());
    let reply = session
        .connection
        .call_method(Some(SERVICE), session.collection.as_str(), Some("org.freedesktop.Secret.Collection"), "CreateItem", &(properties, secret, true))
        .await
        .map_err(error)?;
    let (_item, prompt): (OwnedObjectPath, OwnedObjectPath) = reply.body().deserialize().map_err(error)?;
    if prompt.as_str() != NO_PROMPT {
        return Err("служба секретов запросила подтверждение".into());
    }
    Ok(())
}

async fn remove(name: String) -> Result<(), String> {
    let session = Session::open().await?;
    for item in session.find(&name).await? {
        session.connection.call_method(Some(SERVICE), item.as_str(), Some("org.freedesktop.Secret.Item"), "Delete", &()).await.map_err(error)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod fake {
    use super::SecretStore;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// A wallet in memory. `broken` makes every call fail, like a missing or locked wallet.
    #[derive(Default)]
    pub struct Memory {
        pub items: RefCell<HashMap<String, String>>,
        pub broken: bool,
        pub writes: RefCell<usize>,
    }

    impl SecretStore for Memory {
        fn get(&self, name: &str) -> Result<Option<String>, String> {
            if self.broken { return Err("кошелёк заблокирован".into()); }
            Ok(self.items.borrow().get(name).cloned())
        }
        fn set(&self, name: &str, value: &str) -> Result<(), String> {
            if self.broken { return Err("кошелёк заблокирован".into()); }
            *self.writes.borrow_mut() += 1;
            self.items.borrow_mut().insert(name.to_owned(), value.to_owned());
            Ok(())
        }
        fn remove(&self, name: &str) -> Result<(), String> {
            if self.broken { return Err("кошелёк заблокирован".into()); }
            self.items.borrow_mut().remove(name);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to the real wallet of the session: `cargo test -p lipa-core -- --ignored real_wallet`.
    /// Uses an item name of its own and removes it again.
    #[test]
    #[ignore = "needs a running Secret Service (ksecretd, gnome-keyring)"]
    fn real_wallet_round_trip() {
        let wallet = Wallet;
        let name = "lipax_selftest_key";
        wallet.remove(name).unwrap();
        assert_eq!(wallet.get(name).unwrap(), None);
        wallet.set(name, "первое значение").unwrap();
        assert_eq!(wallet.get(name).unwrap().as_deref(), Some("первое значение"));
        wallet.set(name, "second").unwrap();
        assert_eq!(wallet.get(name).unwrap().as_deref(), Some("second"));
        wallet.remove(name).unwrap();
        assert_eq!(wallet.get(name).unwrap(), None);
    }
}
