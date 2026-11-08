
// ---- appended by scripts/windows/frozen-repro.sh to minutes.rs of the frozen SHA 889d48c (round 2) ----
// Criteria 29-31 through the frozen provider client. The frozen provider URLs are constants, so no
// call is made here (no real host is contacted): the test reads the deadlines the frozen `agent()`
// carries. A client with no connect and no overall deadline lets a stalled provider hold a session
// in `processing` for as long as the server likes.
#[cfg(test)]
mod frozen_repro_minutes {
    use super::*;

    #[test]
    fn the_provider_client_carries_a_finite_connect_and_overall_deadline() {
        let agent = agent();
        let timeouts = agent.config().timeouts();

        assert!(
            timeouts.connect.is_some() && timeouts.global.is_some(),
            "no finite deadline: connect {:?}, global {:?}",
            timeouts.connect,
            timeouts.global
        );
    }
}
