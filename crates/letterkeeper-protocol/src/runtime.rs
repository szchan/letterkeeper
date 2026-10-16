// SPDX-License-Identifier: GPL-3.0-only
use crate::ProtocolError;
use std::{future::Future, time::Duration};
const DEADLINE: Duration = Duration::from_secs(15);
pub(crate) fn run<T: Send>(
    future: impl Future<Output = Result<T, ProtocolError>> + Send,
) -> Result<T, ProtocolError> {
    // spawn_blocking also inherits Tokio's entered handle. Bridge to a fresh
    // scoped thread so block_on cannot nest inside an async runtime context.
    // Async hosts still use spawn_blocking to keep their executor responsive.
    if tokio::runtime::Handle::try_current().is_ok() {
        return std::thread::scope(|scope| {
            let thread = std::thread::Builder::new()
                .name("letterkeeper-protocol".into())
                .spawn_scoped(scope, move || run(future))
                .map_err(|_| ProtocolError::Connection)?;
            thread.join().map_err(|_| ProtocolError::Connection)?
        });
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ProtocolError::Connection)?;
    rt.block_on(async {
        tokio::time::timeout(DEADLINE, future)
            .await
            .map_err(|_| ProtocolError::Timeout)?
    })
}
