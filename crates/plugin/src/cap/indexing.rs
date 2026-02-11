use crate::asset::BoxError;
use crate::model::ParsedFile;
use crate::resolver::ProjectContext;
use crate::ResolvedUnit;

pub trait SourceIndexCap: Send + Sync {
    fn compile_source(&self, file: &ParsedFile, context: &ProjectContext) -> Result<ResolvedUnit, BoxError>;
}

pub trait BuildIndexCap: Send + Sync {
    fn compile_build(&self, files: &[&ParsedFile]) -> Result<(ResolvedUnit, ProjectContext), BoxError>;
}
