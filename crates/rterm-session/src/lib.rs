pub mod automation;
pub mod manager;
pub mod screen_diff;
pub mod session;

pub use automation::resolve_key;
pub use manager::{SessionManager, generate_session_name};
pub use screen_diff::{PrevScreen, pack_color, snapshot};
pub use session::{ManagedSession, SessionState, session_output_loop};
