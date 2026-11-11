// In `remote.rs`, where `Remote`'s fields are visible.
fn baritone_mining_delay(game: &mut crate::game::Game<'_>) {
    if crate::game::baritone::attacking()
        && let Some(r) = game.remote.as_mut()
    {
        r.destroy_delay = 0;
    }
}
