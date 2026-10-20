use std::fmt;

use crate::controller::Advertiser;

/// Everything the laptop shows to the phone while offline: the BLE beacon
/// and the GATT server behind it. Dropping the handle stops both.
pub struct Presence<B: Advertiser, L: Advertiser> {
    pub beacon: B,
    pub link: L,
}

#[derive(Debug)]
pub enum PresenceError<B, L> {
    Beacon(B),
    Link(L),
}

impl<B: fmt::Display, L: fmt::Display> fmt::Display for PresenceError<B, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PresenceError::Beacon(e) => write!(f, "beacon: {e}"),
            PresenceError::Link(e) => write!(f, "link: {e}"),
        }
    }
}

impl<B: Advertiser, L: Advertiser> Advertiser for Presence<B, L> {
    type Handle = (B::Handle, L::Handle);
    type Error = PresenceError<B::Error, L::Error>;

    /// Start the GATT server first so it is ready by the time the beacon
    /// is visible. If the beacon fails, the link handle is dropped.
    async fn start(&self) -> Result<Self::Handle, Self::Error> {
        let link = self.link.start().await.map_err(PresenceError::Link)?;
        let beacon = self.beacon.start().await.map_err(PresenceError::Beacon)?;
        Ok((beacon, link))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    type Log = Arc<Mutex<Vec<String>>>;

    struct FakeHandle {
        name: &'static str,
        log: Log,
    }

    impl Drop for FakeHandle {
        fn drop(&mut self) {
            self.log.lock().unwrap().push(format!("drop:{}", self.name));
        }
    }

    struct Fake {
        name: &'static str,
        log: Log,
        fail: bool,
    }

    impl Advertiser for Fake {
        type Handle = FakeHandle;
        type Error = String;

        async fn start(&self) -> Result<FakeHandle, String> {
            self.log.lock().unwrap().push(format!("start:{}", self.name));
            if self.fail {
                return Err(format!("{} failed", self.name));
            }
            Ok(FakeHandle { name: self.name, log: self.log.clone() })
        }
    }

    fn presence(beacon_fails: bool, link_fails: bool) -> (Presence<Fake, Fake>, Log) {
        let log: Log = Arc::default();
        let p = Presence {
            beacon: Fake { name: "beacon", log: log.clone(), fail: beacon_fails },
            link: Fake { name: "link", log: log.clone(), fail: link_fails },
        };
        (p, log)
    }

    fn events(log: &Log) -> Vec<String> {
        log.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn starts_link_before_beacon() {
        let (p, log) = presence(false, false);
        let Ok(_handle) = p.start().await else { panic!("start failed") };
        assert_eq!(events(&log), ["start:link", "start:beacon"]);
    }

    #[tokio::test]
    async fn dropping_the_handle_stops_both() {
        let (p, log) = presence(false, false);
        let Ok(handle) = p.start().await else { panic!("start failed") };
        drop(handle);
        let e = events(&log);
        assert!(e.contains(&"drop:link".to_string()));
        assert!(e.contains(&"drop:beacon".to_string()));
    }

    #[tokio::test]
    async fn link_failure_never_starts_the_beacon() {
        let (p, log) = presence(false, true);
        assert!(matches!(p.start().await, Err(PresenceError::Link(_))));
        assert_eq!(events(&log), ["start:link"]);
    }

    #[tokio::test]
    async fn beacon_failure_drops_the_link_handle() {
        let (p, log) = presence(true, false);
        assert!(matches!(p.start().await, Err(PresenceError::Beacon(_))));
        assert_eq!(events(&log), ["start:link", "start:beacon", "drop:link"]);
    }

    #[test]
    fn error_message_names_the_failing_part() {
        let e: PresenceError<String, String> = PresenceError::Beacon("no adapter".into());
        assert_eq!(e.to_string(), "beacon: no adapter");
    }
}
