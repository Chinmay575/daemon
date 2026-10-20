use link::{Credentials, Status};
use tokio::sync::watch;
use tracing::{info, warn};

/// Something that can join a Wi-Fi network.
pub trait Connector {
    type Error: std::fmt::Display;

    async fn connect(&self, ssid: &str, password: &str) -> Result<(), Self::Error>;
}

pub struct NmcliConnector;

impl Connector for NmcliConnector {
    type Error = wifi::WifiError;

    async fn connect(&self, ssid: &str, password: &str) -> Result<(), Self::Error> {
        wifi::connect(ssid, password).await
    }
}

/// Joins the network and publishes progress: Connecting, then Connected or Failed.
/// The password is never logged.
pub async fn provision<C: Connector>(
    connector: &C,
    creds: &Credentials,
    status: &watch::Sender<Status>,
) {
    status.send_replace(Status::Connecting);
    match connector.connect(&creds.ssid, &creds.password).await {
        Ok(()) => {
            info!(ssid = %creds.ssid, "wifi connected");
            status.send_replace(Status::Connected);
        }
        Err(e) => {
            warn!(ssid = %creds.ssid, error = %e, "wifi connect failed");
            status.send_replace(Status::Failed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Fake {
        fail: bool,
        status_rx: watch::Receiver<Status>,
        seen_during_connect: Arc<Mutex<Option<Status>>>,
        called_with: Arc<Mutex<Option<(String, String)>>>,
    }

    impl Connector for Fake {
        type Error = String;

        async fn connect(&self, ssid: &str, password: &str) -> Result<(), String> {
            *self.seen_during_connect.lock().unwrap() = Some(*self.status_rx.borrow());
            *self.called_with.lock().unwrap() = Some((ssid.to_string(), password.to_string()));
            if self.fail { Err("no such network".into()) } else { Ok(()) }
        }
    }

    struct Setup {
        fake: Fake,
        tx: watch::Sender<Status>,
        rx: watch::Receiver<Status>,
        seen: Arc<Mutex<Option<Status>>>,
        called: Arc<Mutex<Option<(String, String)>>>,
    }

    fn setup(fail: bool) -> Setup {
        let (tx, rx) = watch::channel(Status::Idle);
        let seen = Arc::new(Mutex::new(None));
        let called = Arc::new(Mutex::new(None));
        let fake = Fake {
            fail,
            status_rx: rx.clone(),
            seen_during_connect: seen.clone(),
            called_with: called.clone(),
        };
        Setup { fake, tx, rx, seen, called }
    }

    fn creds() -> Credentials {
        Credentials { ssid: "phone".into(), password: "secret123".into() }
    }

    #[tokio::test]
    async fn success_ends_connected() {
        let s = setup(false);
        provision(&s.fake, &creds(), &s.tx).await;
        assert_eq!(*s.rx.borrow(), Status::Connected);
    }

    #[tokio::test]
    async fn failure_ends_failed() {
        let s = setup(true);
        provision(&s.fake, &creds(), &s.tx).await;
        assert_eq!(*s.rx.borrow(), Status::Failed);
    }

    #[tokio::test]
    async fn status_is_connecting_while_connect_runs() {
        let s = setup(false);
        provision(&s.fake, &creds(), &s.tx).await;
        assert_eq!(*s.seen.lock().unwrap(), Some(Status::Connecting));
    }

    #[tokio::test]
    async fn connector_receives_ssid_and_password() {
        let s = setup(false);
        provision(&s.fake, &creds(), &s.tx).await;
        assert_eq!(
            *s.called.lock().unwrap(),
            Some(("phone".to_string(), "secret123".to_string()))
        );
    }

    #[tokio::test]
    async fn can_provision_again_after_failure() {
        let s = setup(true);
        provision(&s.fake, &creds(), &s.tx).await;
        provision(&s.fake, &creds(), &s.tx).await;
        assert_eq!(*s.rx.borrow(), Status::Failed);
    }
}
