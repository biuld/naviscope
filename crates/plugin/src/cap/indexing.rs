use crate::ResolvedUnit;
use crate::asset::BoxError;
use crate::indexing::ProjectContext;
use crate::model::ParsedFile;
use std::any::Any;

pub trait SourceCollectArtifact: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync>;
    fn collected_type_symbols(&self) -> &[String];
    fn collected_method_symbols(&self) -> &[String];
    fn provided_dependency_symbols(&self) -> &[String];
    fn required_dependency_symbols(&self) -> &[String];
}

pub trait SourceAnalyzeArtifact: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync>;
}

pub trait SourceIndexCap: Send + Sync {
    fn collect_source(
        &self,
        file: &ParsedFile,
        context: &ProjectContext,
    ) -> Result<Box<dyn SourceCollectArtifact>, BoxError>;

    fn analyze_source(
        &self,
        collected: Box<dyn SourceCollectArtifact>,
        context: &ProjectContext,
    ) -> Result<Box<dyn SourceAnalyzeArtifact>, BoxError>;

    fn lower_source(
        &self,
        analyzed: Box<dyn SourceAnalyzeArtifact>,
        context: &ProjectContext,
    ) -> Result<ResolvedUnit, BoxError>;
}

pub trait BuildIndexCap: Send + Sync {
    fn compile_build(
        &self,
        files: &[&ParsedFile],
    ) -> Result<(ResolvedUnit, ProjectContext), BoxError>;
}
