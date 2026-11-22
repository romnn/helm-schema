use color_eyre::eyre;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::{Event, Reader, Syntax, Token, TokenKind};

fn observe(input: &str) -> eyre::Result<(Vec<Token>, Vec<Event>)> {
    let mut reader = Reader::default();
    let mut tokens = Vec::new();
    let mut events = Vec::new();
    let mut observer = |syntax: Syntax<'_>| {
        match syntax {
            Syntax::Token(token) => tokens.push(token.clone()),
            Syntax::Event(event) => events.push(event.clone()),
        }
        Ok(())
    };
    for character in input.chars() {
        reader.step_syntax(character, &mut observer)?;
    }
    reader.finish_syntax(&mut observer)?;
    Ok((tokens, events))
}

fn events(input: &str) -> eyre::Result<Vec<Event>> {
    let mut reader = Reader::default();
    let mut events = Vec::new();
    let mut sink = |event: &Event| {
        events.push(event.clone());
        Ok(())
    };
    for character in input.chars() {
        reader.step(character, &mut sink)?;
    }
    reader.finish(&mut sink)?;
    Ok(events)
}

#[test]
fn observed_events_equal_the_event_sink_and_tokens_arrive_in_document_order() -> eyre::Result<()> {
    let input = indoc! {"
        a: [x, {y: z}]
        b:
        - c
        - d: e
    "};
    let (tokens, observed) = observe(input)?;
    sim_assert_eq!(have: observed, want: events(input)?);
    let entries: Vec<usize> = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::BlockEntry)
        .map(|token| token.start.index)
        .collect();
    sim_assert_eq!(have: entries, want: vec![18, 22]);
    let starts: Vec<usize> = tokens.iter().map(|token| token.start.index).collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    sim_assert_eq!(have: starts, want: sorted);
    Ok(())
}
