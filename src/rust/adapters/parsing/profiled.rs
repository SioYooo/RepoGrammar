//! Diagnostic decorator; delegates every parser/session operation unchanged.
use super::RepoGrammarSourceParser;
use crate::ports::host_profile::{HostPhase, HostProfile};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, ParserProjectSession, PythonInterfaceProbe,
    SourceDocument, SourceParseOutput, SourceParser,
};

pub struct ProfiledParser {
    pub parser: RepoGrammarSourceParser,
    pub profile: HostProfile,
}
impl SourceParser for ProfiledParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        let _span = self.profile.span(HostPhase::Analyzer);
        self.profile.work(HostPhase::Analyzer, 1, 0);
        self.parser.parse(document)
    }
    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        let _span = self.profile.span(HostPhase::Analyzer);
        self.profile.work(HostPhase::Analyzer, 1, 0);
        self.parser.parse_with_context(document, context)
    }
    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        let _span = self.profile.span(HostPhase::Analyzer);
        self.profile.work(HostPhase::Analyzer, 1, 0);
        self.parser.parse_with_context_output(document, context)
    }
    fn begin_project_session(
        &self,
        context: &ParserProjectContext,
    ) -> Result<Option<Box<dyn ParserProjectSession>>, ParseError> {
        let _span = self.profile.span(HostPhase::Analyzer);
        self.profile.work(HostPhase::Analyzer, 1, 0);
        self.parser.begin_project_session(context).map(|session| {
            if session.is_some() {
                self.profile.note_python_session();
            }
            session.map(|inner| {
                Box::new(ProfiledSession {
                    inner,
                    profile: self.profile.clone(),
                }) as Box<dyn ParserProjectSession>
            })
        })
    }
    fn extract_python_interface(&self, path: &str, text: &str) -> PythonInterfaceProbe {
        let _span = self.profile.span(HostPhase::Analyzer);
        self.profile.work(HostPhase::Analyzer, 1, 0);
        self.parser.extract_python_interface(path, text)
    }
    fn python_frontend_version(&self) -> Option<String> {
        self.parser.python_frontend_version()
    }
}

struct ProfiledSession {
    inner: Box<dyn ParserProjectSession>,
    profile: HostProfile,
}
impl ParserProjectSession for ProfiledSession {
    fn parse(&mut self, document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
        let _span = self.profile.span(HostPhase::Analyzer);
        self.profile.work(HostPhase::Analyzer, 1, 0);
        self.inner.parse(document)
    }
    fn finish(&mut self) -> Result<(), ParseError> {
        let _span = self.profile.span(HostPhase::Analyzer);
        self.profile.work(HostPhase::Analyzer, 1, 0);
        self.inner.finish()
    }
}
