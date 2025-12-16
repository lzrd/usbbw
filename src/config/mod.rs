//! Configuration loading and management.

mod loader;

pub use loader::{
    Config, ConfigError, MermaidConfig, PhysicalPortLabel, PositionLabels, Settings,
    example_config, generate_config,
};
