use std::sync::mpsc;
use std::time::Duration;

use color_eyre::eyre::{self, WrapErr as _};
use test_util::prelude::sim_assert_eq;

use crate::session::{InitializingPhases, SessionCache, SessionPhase};

#[test]
fn session_cache_reentrant_initializer_fails_fast() -> eyre::Result<()> {
    let (sender, receiver) = mpsc::channel();
    let initializer = std::thread::spawn(move || {
        let phases = InitializingPhases::default();
        let cache = SessionCache::<u32>::new(SessionPhase::ResolvedContract);
        let outer = cache.get_or_try_init(&phases, || {
            let inner = cache.get_or_try_init(&phases, || Ok(1));
            // A failed send only means the test already timed out.
            let _ = sender.send(inner.err().map(|error| error.to_string()));
            Ok(2)
        });
        let after = cache.get_or_try_init(&phases, || Ok(3));
        (
            outer.map(|value| *value).map_err(|error| error.to_string()),
            after.map(|value| *value).map_err(|error| error.to_string()),
        )
    });

    let inner = receiver
        .recv_timeout(Duration::from_secs(10))
        .wrap_err("the re-entrant session query did not return")?;
    let (outer, after) = initializer
        .join()
        .map_err(|_| eyre::eyre!("the initializing thread panicked"))?;

    sim_assert_eq!(
        have: inner,
        want: Some(
            "session query for the resolved contract phase re-entered the session while \
             initializing the resolved contract phase"
                .to_string()
        )
    );
    sim_assert_eq!(have: outer, want: Ok(2));
    sim_assert_eq!(have: after, want: Ok(2));
    Ok(())
}
