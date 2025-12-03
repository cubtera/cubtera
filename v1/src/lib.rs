pub mod core;
pub mod utils;

mod globals {
    pub use crate::core::cfg::*;
}

pub mod prelude {
    pub use crate::core::dim::*;
    pub use crate::core::dlog::*;
    pub use crate::core::im::*;
    pub use crate::core::runner::*;
    pub use crate::core::unit::*;
    pub use crate::globals::GLOBAL_CFG;
    pub use crate::utils::helper::*;
    pub use log::*;
}
