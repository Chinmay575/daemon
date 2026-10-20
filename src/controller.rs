use tracing::{info, warn};

/// Something that can start advertising. Dropping the handle stops it.
pub trait Advertiser {
    type Handle;
    type Error: std::fmt::Display;

    async fn start(&self) -> Result<Self::Handle, Self::Error>;
}

impl Advertiser for beacon::Beacon {
    type Handle = beacon::BeaconHandle;
    type Error = beacon::BeaconError;

    async fn start(&self) -> Result<Self::Handle, Self::Error> {
        beacon::Beacon::start(self).await
    }
}

impl Advertiser for link::Link {
    type Handle = link::LinkHandle;
    type Error = link::LinkError;

    async fn start(&self) -> Result<Self::Handle, Self::Error> {
        link::Link::start(self).await
    }
}

/// Keeps the beacon on or off as asked, and never starts it twice.
pub struct Controller<A: Advertiser> {
    advertiser: A,
    handle: Option<A::Handle>,
}

impl<A: Advertiser> Controller<A> {
    pub fn new(advertiser: A) -> Self {
        Self {
            advertiser,
            handle: None,
        }
    }

    /// A failed start is logged and leaves the beacon off; the next request
    /// to turn it on tries again.
    pub async fn set_wanted(&mut self, wanted: bool) {
        match (wanted, self.handle.is_some()) {
            (true, false) => match self.advertiser.start().await {
                Ok(handle) => {
                    info!("beacon on");
                    self.handle = Some(handle);
                }
                Err(e) => warn!(error = %e, "could not start beacon"),
            },
            (false, true) => {
                self.handle = None;
                info!("beacon off");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeHandle(Arc<AtomicUsize>);

    impl Drop for FakeHandle {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[derive(Default)]
    struct Fake {
        starts: Arc<AtomicUsize>,
        live: Arc<AtomicUsize>,
        fail: bool,
    }

    impl Advertiser for Fake {
        type Handle = FakeHandle;
        type Error = String;

        async fn start(&self) -> Result<FakeHandle, String> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err("no adapter".into());
            }
            self.live.fetch_add(1, Ordering::SeqCst);
            Ok(FakeHandle(self.live.clone()))
        }
    }

    #[tokio::test]
    async fn turning_on_twice_starts_once() {
        let fake = Fake::default();
        let (starts, live) = (fake.starts.clone(), fake.live.clone());
        let mut c = Controller::new(fake);

        c.set_wanted(true).await;
        c.set_wanted(true).await;

        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(live.load(Ordering::SeqCst), 1);

    }

    #[tokio::test]
    async fn turning_off_drops_the_handle() {
        let fake = Fake::default();
        let live = fake.live.clone();
        let mut c = Controller::new(fake);

        c.set_wanted(true).await;
        c.set_wanted(false).await;

        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn turning_off_while_off_does_nothing() {
        let fake = Fake::default();
        let starts = fake.starts.clone();
        let mut c = Controller::new(fake);

        c.set_wanted(false).await;

        assert_eq!(starts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rapid_toggling_never_leaks_a_handle() {
        let fake = Fake::default();
        let live = fake.live.clone();
        let mut c = Controller::new(fake);

        for i in 0..50 {
            c.set_wanted(i % 2 == 0).await;
            assert!(live.load(Ordering::SeqCst) <= 1);
        }
        c.set_wanted(false).await;

        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn failed_start_leaves_it_off_and_retries_next_time() {
        let fake = Fake {
            fail: true,
            ..Fake::default()
        };
        let starts = fake.starts.clone();
        let mut c = Controller::new(fake);

        c.set_wanted(true).await;
        c.set_wanted(true).await;

        assert_eq!(starts.load(Ordering::SeqCst), 2);
    }
}
