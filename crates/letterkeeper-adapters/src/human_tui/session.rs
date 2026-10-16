// SPDX-License-Identifier: GPL-3.0-only
use super::*;
pub fn run_tui_workspace(
    host: Arc<dyn Host>,
    workspace: Arc<dyn HumanWorkspace>,
) -> io::Result<()> {
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "human workspace requires an interactive terminal",
        ));
    }
    let mut state = WorkspaceState::new(workspace.capabilities());
    let mut worker = WorkspaceWorker::new(host, workspace);
    let _guard = TerminalGuard::enter()?;
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(io::stdout()))?;
    let mut redraw = true;
    while !state.quit {
        redraw |= worker.poll(&mut state);
        if redraw {
            terminal.draw(|frame| render_workspace(frame, &mut state))?;
            redraw = false;
        }
        if event::poll(std::time::Duration::from_millis(20))? {
            state.event(event::read()?);
            redraw = true;
        }
    }
    Ok(())
}
