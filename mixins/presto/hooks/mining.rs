// In `remote.rs`, where `Remote`'s fields are visible.
fn presto_mining_delay(game: &mut crate::game::Game<'_>) {
    if crate::game::presto::attacking()
        && let Some(r) = game.remote.as_mut()
    {
        r.destroy_delay = 0;
    }
}
