use crate::GradlePlugin;
use naviscope_api::models::BuildTool;
use naviscope_plugin::BuildCaps;
use std::sync::Arc;

pub fn gradle_caps() -> BuildCaps {
    let plugin = Arc::new(GradlePlugin::new());
    BuildCaps {
        build_tool: BuildTool::GRADLE,
        matcher: plugin.clone(),
        parser: plugin.clone(),
        indexing: plugin.clone(),
        asset: plugin.clone(),
        presentation: plugin.clone(),
        metadata_codec: plugin,
    }
}
