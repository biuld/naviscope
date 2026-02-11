use crate::GradlePlugin;
use naviscope_plugin::BuildIndexCap;

impl BuildIndexCap for GradlePlugin {
    fn compile_build(
        &self,
        files: &[&naviscope_plugin::ParsedFile],
    ) -> Result<
        (
            naviscope_plugin::ResolvedUnit,
            naviscope_plugin::ProjectContext,
        ),
        naviscope_plugin::BoxError,
    > {
        let resolver = crate::resolve::GradleResolver::new();
        resolver.compile_build(files)
    }
}
