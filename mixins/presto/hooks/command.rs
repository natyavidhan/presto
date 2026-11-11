fn presto_command(command: &str, ran: &mut bool) -> std::ops::ControlFlow<()> {
    if crate::game::presto::command(command) {
        *ran = true;
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
