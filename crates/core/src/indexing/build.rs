use crate::error::Result;
use crate::indexing::scanner::ParsedFile;
use crate::model::GraphOp;
use naviscope_plugin::{BuildCaps, BuildContent, ParsedContent, ProjectContext};
use std::fs;

pub struct BuildCompiler {
    build_caps: Vec<BuildCaps>,
}

impl BuildCompiler {
    pub fn with_caps(build_caps: Vec<BuildCaps>) -> Self {
        Self { build_caps }
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
                let parsed_tool_files: Vec<ParsedFile> = tool_files
                    .iter()
                    .map(|f| Self::prepare_build_file(caps, f))
                    .collect::<Result<Vec<_>>>()?;
                let parsed_tool_file_refs: Vec<&ParsedFile> = parsed_tool_files.iter().collect();
                let (unit, ctx) = caps
                    .indexing
                    .compile_build(&parsed_tool_file_refs)
                    .map_err(crate::error::NaviscopeError::from)?;
                all_ops.extend(unit.ops);
                context.path_to_module.extend(ctx.path_to_module);
            }
        }
        Ok(all_ops)
    }

    fn prepare_build_file(caps: &BuildCaps, file: &ParsedFile) -> Result<ParsedFile> {
        let source = match &file.content {
            ParsedContent::Unparsed(s) => s.clone(),
            ParsedContent::Lazy => fs::read_to_string(file.path()).map_err(|e| {
                crate::error::NaviscopeError::Internal(format!(
                    "Failed to read build file {}: {}",
                    file.path().display(),
                    e
                ))
            })?,
            ParsedContent::Metadata(_) => return Ok(file.clone()),
            ParsedContent::Language(_) => return Ok(file.clone()),
        };

        let parse_result = caps
            .parser
            .parse_build_file(&source)
            .map_err(crate::error::NaviscopeError::from)?;

        let content = match parse_result.content {
            BuildContent::Metadata(value) => ParsedContent::Metadata(value),
            BuildContent::Unparsed(text) => ParsedContent::Unparsed(text),
            // Build indexing currently consumes Metadata/Unparsed; preserve source for this case.
            BuildContent::Parsed(_) => ParsedContent::Unparsed(source),
        };

        Ok(ParsedFile {
            file: file.file.clone(),
            content,
        })
    }
}
