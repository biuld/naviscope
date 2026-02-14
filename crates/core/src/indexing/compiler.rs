use crate::error::Result;
use crate::indexing::scanner::ParsedFile;
use crate::model::{GraphOp, ResolvedUnit};
use naviscope_plugin::{BuildCaps, LanguageCaps, ProjectContext};

pub struct BatchCompiler {
    build_caps: Vec<BuildCaps>,
    lang_caps: Vec<LanguageCaps>,
}

impl BatchCompiler {
    pub fn with_caps(build_caps: Vec<BuildCaps>, lang_caps: Vec<LanguageCaps>) -> Self {
        Self {
            build_caps,
            lang_caps,
        }
    }

    pub fn compile_build_batch(
        &self,
        build_files: &[ParsedFile],
        context: &mut ProjectContext,
    ) -> Result<Vec<GraphOp>> {
        let mut all_ops = Vec::new();
        for caps in &self.build_caps {
            let tool_files: Vec<&ParsedFile> = build_files
                .iter()
                .filter(|f| caps.matcher.supports_path(f.path()))
                .collect();

            if !tool_files.is_empty() {
                let (unit, ctx) = caps
                    .indexing
                    .compile_build(&tool_files)
                    .map_err(crate::error::NaviscopeError::from)?;
                all_ops.extend(unit.ops);
                context.path_to_module.extend(ctx.path_to_module);
            }
        }
        Ok(all_ops)
    }

    pub fn compile_source_batch(
        &self,
        source_files: &[ParsedFile],
        context: &ProjectContext,
    ) -> Result<Vec<GraphOp>> {
        let source_results: Vec<Result<ResolvedUnit>> = source_files
            .iter()
            .map(|file| {
                let caps = self
                    .lang_caps
                    .iter()
                    .find(|c| c.matcher.supports_path(file.path()));

                if let Some(c) = caps {
                    c.indexing
                        .compile_source(file, context)
                        .map_err(crate::error::NaviscopeError::from)
                } else {
                    Ok(ResolvedUnit::new())
                }
            })
            .collect();

        let mut all_ops = Vec::new();
        for result in source_results {
            let unit = result?;
            all_ops.extend(unit.ops);
        }
        Ok(all_ops)
    }
}
