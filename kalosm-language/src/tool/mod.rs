mod search;
use std::borrow::Cow;

use kalosm_sample::{
    ChoiceParser, CreateParserState, Either, LiteralParser, LiteralParserOffset, ParseResult,
    Parser, SequenceParser, SequenceParserState,
};
pub use search::*;
mod calculator;
pub use calculator::*;
mod document;
pub use document::*;

/// A tool that can be used by a [`kalosm_language_model::Model`]
// TODO: Add example
#[async_trait::async_trait]
pub trait Tool {
    /// The name of the tool
    fn name(&self) -> String;
    /// The prompt for the input to the tool
    fn input_prompt(&self) -> String;
    /// A description of the tool
    fn description(&self) -> String;
    /// Run the tool with the given arguments
    async fn run(&mut self, args: &str) -> String;
}

/// A set of tools that can be used by a [`kalosm_language_model::Model`]
#[derive(Default)]
pub struct ToolManager {
    tools: Vec<Box<dyn Tool + Send + Sync>>,
}

impl std::fmt::Debug for ToolManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolManager")
            .field(
                "tools",
                &self.tools.iter().map(|t| t.name()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ToolManager {
    /// Create a new tool empty manager
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Add a tool to the manager
    pub fn with_tool(self, tool: impl Tool + Send + Sync + 'static) -> Self {
        let mut tools = self.tools;
        tools.push(Box::new(tool));
        Self { tools }
    }

    /// Add a tool to the manager
    pub fn add_tool(&mut self, tool: impl Tool + Send + Sync + 'static) {
        self.tools.push(Box::new(tool));
    }

    /// Get the tools in the manager
    pub fn get_tools(&self) -> &[Box<dyn Tool + Send + Sync>] {
        &self.tools
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&(dyn Tool + Send + Sync)> {
        self.tools.iter().find(|t| t.name() == name).map(|t| &**t)
    }

    /// Get a tool mutably by name
    pub fn get_tool_mut<'a>(&'a mut self, name: &str) -> Option<&'a mut (dyn Tool + Send + Sync)> {
        for tool in &mut self.tools {
            if tool.name() == name {
                return Some(&mut **tool);
            }
        }
        None
    }

    /// Get a tool by index
    pub fn get_tool_by_index(&self, index: usize) -> Option<&(dyn Tool + Send + Sync)> {
        self.tools.get(index).map(|t| &**t)
    }

    /// Get a tool mutably by index
    pub fn get_tool_mut_by_index(&mut self, index: usize) -> Option<&mut (dyn Tool + Send + Sync)> {
        match self.tools.get_mut(index) {
            Some(tool) => Some(&mut **tool),
            None => None,
        }
    }

    /// Get a prompt for the tools in the manager
    pub fn prompt(&self, question: impl std::fmt::Display) -> String {
        let mut tools = String::new();
        let mut tool_names = String::new();
        for tool in self.tools.iter() {
            tools.push_str(&format!("# {}\n{}", tool.name(), tool.description()));
            tool_names.push_str(&format!("'{}'", tool.name()));
        }
        format!(
            r#"Use the following format:

Question: the input question you must answer
Thought: you should always think about what to do
Action: the action to take, should be one of [{tool_names}]
Input: the input to the action
Observation: the result of the action
... (this Thought/Action/Input/Observation can repeat N times)
Thought: I now know the final answer
Final Answer: the final answer to the original input question

You have access to the following tools:

{tools}

Begin!

Question: {question}
"#
        )
    }

    /// Get the constraints for the tools in the manager
    pub fn tool_choices(
        &self,
    ) -> Option<
        impl Parser<
                Error = (),
                Output = usize,
                PartialState = IndexParserState<LiteralParserOffset, ()>,
            > + CreateParserState
            + Send
            + Sync
            + 'static,
    > {
        let mut choices: Vec<LiteralParser<_>> = Vec::with_capacity(self.tools.len());
        for tool in self.tools.iter() {
            let name = tool.name();
            let prompt = tool.input_prompt();
            choices.push(LiteralParser::from(format!("{name}\n{prompt}")));
        }
        if choices.is_empty() {
            None
        } else {
            Some(IndexParser { parsers: choices })
        }
    }

    /// Get the constraints for the thought action
    pub fn thought_constraints(
        &self,
    ) -> impl Parser<
        Error = Either<(), ()>,
        Output = ((), String),
        PartialState = SequenceParserState<LiteralParserOffset, OneLineState, ()>,
    > + CreateParserState
           + Send
           + Sync
           + 'static {
        let constraints = "Thought: ";
        let constraints = LiteralParser::from(constraints).then(OneLine);
        constraints
    }

    /// Get the constraints for the action action
    pub fn action_constraints(
        &self,
    ) -> SequenceParser<
        SequenceParser<
            LiteralParser<&'static str>,
            impl Parser<
                    Error = (),
                    Output = usize,
                    PartialState = IndexParserState<LiteralParserOffset, ()>,
                > + CreateParserState
                + Send
                + Sync
                + 'static,
        >,
        OneLine,
    > {
        let constraints = LiteralParser::from("Action: ");
        let constraints = constraints.then(self.tool_choices().unwrap());
        let constraints = constraints.then(OneLine);
        constraints
    }

    /// Get the constraints for the answer action
    pub fn answer_constraints(
        &self,
    ) -> impl Parser<
        Error = Either<(), ()>,
        Output = ((), String),
        PartialState = SequenceParserState<LiteralParserOffset, OneLineState, ()>,
    > + CreateParserState
           + Send
           + Sync
           + 'static {
        let constraints = LiteralParser::from("Final Answer: ");
        let constraints = constraints.then(OneLine);
        constraints
    }

    /// Get the constraints for any action
    pub fn any_action_constraint(
        &self,
    ) -> ChoiceParser<
        ChoiceParser<
            impl kalosm_sample::Parser<
                    Error = kalosm_sample::Either<(), ()>,
                    Output = ((), std::string::String),
                    PartialState = SequenceParserState<LiteralParserOffset, OneLineState, ()>,
                > + CreateParserState
                + Send
                + Sync
                + 'static,
            SequenceParser<
                SequenceParser<
                    LiteralParser<&str>,
                    impl kalosm_sample::Parser<
                            Error = (),
                            Output = usize,
                            PartialState = IndexParserState<LiteralParserOffset, ()>,
                        > + CreateParserState
                        + Send
                        + Sync
                        + 'static,
                >,
                OneLine,
            >,
        >,
        impl kalosm_sample::Parser<
                Error = kalosm_sample::Either<(), ()>,
                Output = ((), std::string::String),
                PartialState = SequenceParserState<LiteralParserOffset, OneLineState, ()>,
            > + CreateParserState
            + Send
            + Sync
            + 'static,
    > {
        self.thought_constraints()
            .or(self.action_constraints())
            .or(self.answer_constraints())
    }
}

/// The state of the [`IndexParser`] parser
#[derive(Debug, Clone)]
pub struct IndexParserState<PA, E> {
    states: Vec<Result<PA, E>>,
}

/// A parser that parses a sequence of parsers and returns the index of the first parser that succeeds
pub struct IndexParser<S: Parser<Error = E, Output = (), PartialState = PA>, E, PA> {
    parsers: Vec<S>,
}

impl<S: Parser<Error = E, Output = (), PartialState = PA>, E, PA> IndexParser<S, E, PA> {
    /// Create a new index parser
    pub fn new(parsers: Vec<S>) -> Self {
        Self { parsers }
    }
}

impl<S, E, PA> CreateParserState for IndexParser<S, E, PA>
where
    S: Parser<Error = E, Output = (), PartialState = PA> + CreateParserState,
    E: Clone,
    PA: Clone,
{
    fn create_parser_state(&self) -> Self::PartialState {
        IndexParserState {
            states: self
                .parsers
                .iter()
                .map(|s| Ok(s.create_parser_state()))
                .collect(),
        }
    }
}

impl<S, E, PA> Parser for IndexParser<S, E, PA>
where
    S: Parser<Error = E, Output = (), PartialState = PA>,
    E: Clone,
    PA: Clone,
{
    type Error = E;
    type Output = usize;
    type PartialState = IndexParserState<PA, E>;

    fn parse<'a>(
        &self,
        state: &Self::PartialState,
        input: &'a [u8],
    ) -> Result<kalosm_sample::ParseResult<'a, Self::PartialState, Self::Output>, Self::Error>
    where
        Self: Sized,
    {
        let mut states = state.states.clone();
        let mut has_incomplete_option = false;
        let mut required_next: Option<Cow<'static, str>> = None;
        let last_index = self.parsers.len() - 1;
        for (i, parser) in self.parsers.iter().enumerate() {
            match &states[i] {
                Ok(state) => {
                    let result = parser.parse(state, input);
                    match result {
                        Ok(ParseResult::Finished {
                            result: _,
                            remaining: r,
                        }) => {
                            return Ok(ParseResult::Finished {
                                result: i,
                                remaining: r,
                            })
                        }
                        Ok(ParseResult::Incomplete {
                            new_state: s,
                            required_next: new_required_next,
                        }) => {
                            states[i] = Ok(s);
                            has_incomplete_option = true;
                            match required_next {
                                Some(r) => {
                                    let mut common_bytes = 0;
                                    for (byte1, byte2) in r.bytes().zip(new_required_next.bytes()) {
                                        if byte1 != byte2 {
                                            break;
                                        }
                                        common_bytes += 1;
                                    }
                                    required_next = Some(match (r, new_required_next) {
                                        (Cow::Borrowed(required_next), _) => {
                                            Cow::Borrowed(&required_next[common_bytes..])
                                        }
                                        (_, Cow::Borrowed(required_next)) => {
                                            Cow::Borrowed(&required_next[common_bytes..])
                                        }
                                        (Cow::Owned(mut required_next), _) => {
                                            required_next.truncate(common_bytes);
                                            Cow::Owned(required_next)
                                        }
                                    });
                                }
                                None => {
                                    required_next = Some(new_required_next);
                                }
                            }
                        }
                        Err(e) => {
                            if !has_incomplete_option && i == last_index {
                                return Err(e);
                            }
                            states[i] = Err(e);
                        }
                    }
                }
                Err(err) => {
                    if !has_incomplete_option && i == last_index {
                        return Err(err.clone());
                    }
                }
            }
        }
        Ok(ParseResult::Incomplete {
            new_state: IndexParserState { states },
            required_next: required_next.unwrap_or_default(),
        })
    }
}

/// One line of text with some non-whitespace characters
pub struct OneLine;

/// The state of the [`OneLine`] parser
#[derive(Debug, Clone)]
pub struct OneLineState {
    all_whitespace: bool,
    bytes: Vec<u8>,
}

impl CreateParserState for OneLine {
    fn create_parser_state(&self) -> <Self as Parser>::PartialState {
        OneLineState {
            all_whitespace: true,
            bytes: Vec::new(),
        }
    }
}

impl Parser for OneLine {
    type Error = ();
    type Output = String;
    type PartialState = OneLineState;

    fn parse<'a>(
        &self,
        state: &Self::PartialState,
        input: &'a [u8],
    ) -> Result<kalosm_sample::ParseResult<'a, Self::PartialState, Self::Output>, Self::Error>
    where
        Self: Sized,
    {
        if input.is_empty() {
            if state.all_whitespace {
                return Err(());
            } else {
                return Ok(ParseResult::Incomplete {
                    new_state: state.clone(),
                    required_next: Default::default(),
                });
            }
        }
        let mut state = state.clone();
        let mut iter = input.iter();
        while let Some(&c) = iter.next() {
            if state.all_whitespace {
                if let Ok(c) = char::try_from(c) {
                    if !c.is_whitespace() {
                        state.all_whitespace = false;
                    }
                }
            }
            if c == b'\n' || c == b'\r' {
                if state.all_whitespace {
                    return Err(());
                } else {
                    return Ok(ParseResult::Finished {
                        result: String::from_utf8_lossy(&state.bytes).to_string(),
                        remaining: iter.as_slice(),
                    });
                }
            }
            state.bytes.push(c);
        }
        Ok(ParseResult::Incomplete {
            new_state: state,
            required_next: Default::default(),
        })
    }
}

macro_rules! impl_from_tool_tuple {
    ($($name:ident),*) => {
        #[allow(non_snake_case)]
        impl<$($name: Tool + Send + Sync + 'static),*> From<($($name,)*)> for ToolManager {
            fn from(tools: ($($name,)*)) -> Self {
                let ($($name,)*) = tools;
                Self::new()$(.with_tool($name))*
            }
        }
    };
}

impl_from_tool_tuple!();
impl_from_tool_tuple!(A);
impl_from_tool_tuple!(A, B);
impl_from_tool_tuple!(A, B, C);
impl_from_tool_tuple!(A, B, C, D);
impl_from_tool_tuple!(A, B, C, D, E);
impl_from_tool_tuple!(A, B, C, D, E, F);
impl_from_tool_tuple!(A, B, C, D, E, F, G);
impl_from_tool_tuple!(A, B, C, D, E, F, G, H);
impl_from_tool_tuple!(A, B, C, D, E, F, G, H, I);
impl_from_tool_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_from_tool_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_from_tool_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);
