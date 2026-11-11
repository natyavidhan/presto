// In `chat.rs`: the message as `handleChatInput` normalizes it, and the chat history it would go into.
fn presto_chat(game: &mut crate::game::Game<'_>, message: &str) -> std::ops::ControlFlow<()> {
    let message = normalize_chat_message(message);
    if crate::game::presto::chat(&message) {
        game.ui.chat.add_recent(&message);
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
