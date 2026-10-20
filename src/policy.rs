use netwatch::{ConnectivityState, Update};

/// Whether the beacon should be on for this update.
///
/// `None` means "keep whatever it is doing now": an `Unknown` state or an
/// error is not evidence that the internet came back or went away.
pub fn beacon_wanted(update: &Update) -> Option<bool> {
    match update {
        Ok(ConnectivityState::Full) => Some(false),
        Ok(ConnectivityState::Offline | ConnectivityState::Portal | ConnectivityState::Limited) => {
            Some(true)
        }
        Ok(ConnectivityState::Unknown) | Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netwatch::ConnectivityError;

    #[test]
    fn no_internet_states_turn_the_beacon_on() {
        for state in [
            ConnectivityState::Offline,
            ConnectivityState::Portal,
            ConnectivityState::Limited,
        ] {
            assert_eq!(beacon_wanted(&Ok(state)), Some(true), "{state:?}");
        }
    }

    #[test]
    fn full_internet_turns_the_beacon_off() {
        assert_eq!(beacon_wanted(&Ok(ConnectivityState::Full)), Some(false));
    }

    #[test]
    fn unknown_keeps_the_current_state() {
        assert_eq!(beacon_wanted(&Ok(ConnectivityState::Unknown)), None);
    }

    #[test]
    fn errors_keep_the_current_state() {
        assert_eq!(beacon_wanted(&Err(ConnectivityError::Disconnected)), None);
        assert_eq!(beacon_wanted(&Err(ConnectivityError::UnknownValue(9))), None);
    }
}
