use super::*;
use crate::document::SelectionRange;
use crate::history::EditorState;
use crate::transaction::TextEdit;

/// Parses text where `|` marks a caret and `«…»` marks a selected range.
fn parse(marked: &str) -> (Document, Selection) {
    let mut text = String::new();
    let mut ranges = Vec::new();
    let mut open = None;
    for ch in marked.chars() {
        match ch {
            '|' => ranges.push(SelectionRange::cursor(text.len())),
            '«' => open = Some(text.len()),
            '»' => ranges.push(SelectionRange::new(open.take().unwrap(), text.len())),
            _ => text.push(ch),
        }
    }
    (Document::from(text.as_str()), Selection::new(ranges, 0))
}

/// Renders a document and selection back into the marked form.
fn render(doc: &Document, selection: &Selection) -> String {
    let mut text = doc.to_string();
    for range in selection.ranges().iter().rev() {
        if range.is_empty() {
            text.insert(range.head, '|');
        } else {
            text.insert(range.to(), '»');
            text.insert(range.from(), '«');
        }
    }
    text
}

fn run_in(
    pipeline: &Pipeline,
    marked: &str,
    request: EditRequest,
    context: InputContext,
) -> String {
    let (doc, selection) = parse(marked);
    let mut state = EditorState::new(doc);
    state
        .apply(Transaction::select(selection, Origin::Input, 0))
        .unwrap();
    let Some(transaction) = pipeline.run(
        request,
        state.doc(),
        state.selection(),
        &FixedContext(context),
        1,
    ) else {
        return render(state.doc(), state.selection());
    };
    state.apply(transaction).unwrap();
    render(state.doc(), state.selection())
}

fn run(marked: &str, request: EditRequest) -> String {
    run_in(&Pipeline::builtin(), marked, request, InputContext::Text)
}

fn typed(text: &str) -> EditRequest {
    EditRequest::InsertText(text.to_owned())
}

#[test]
fn default_order() {
    let pipeline = Pipeline::builtin();
    assert_eq!(pipeline.step_names(), step_names::DEFAULT_ORDER);
    assert!(pipeline.slot("snippets").unwrap().is_placeholder());
    assert!(!pipeline.slot("apply").unwrap().is_placeholder());
    let emoji = pipeline.slot("emoji").unwrap().contexts();
    assert!(!emoji.allows(InputContext::Math));
}

#[test]
fn apply_inserts_at_every_caret() {
    assert_eq!(run("a|b|c", typed("x")), "ax|bx|c");
    assert_eq!(run("«ab»c", typed("é")), "é|c");
    assert_eq!(run("a|b", EditRequest::Tab), "a\t|b");
    assert_eq!(run("a|b", EditRequest::Newline), "a\n|b");
}

#[test]
fn apply_deletes() {
    assert_eq!(run("aé|b", EditRequest::DeleteBackward), "a|b");
    assert_eq!(run("|ab", EditRequest::DeleteBackward), "|ab");
    assert_eq!(run("a\r\n|b", EditRequest::DeleteBackward), "a|b");
    assert_eq!(run("a|😀b", EditRequest::DeleteForward), "a|b");
    assert_eq!(run("a|\r\nb", EditRequest::DeleteForward), "a|b");
    assert_eq!(run("ab|", EditRequest::DeleteForward), "ab|");
    assert_eq!(run("«ab»c", EditRequest::DeleteForward), "|c");
    assert_eq!(run("a|b|c", EditRequest::DeleteBackward), "|c");
}

#[test]
fn apply_passes_explicit_changes() {
    let request = EditRequest::Replace {
        changes: ChangeSet::insert(0, "> "),
        selection: None,
    };
    assert_eq!(run("a|", request), "> a|");
}

#[test]
fn list_continues_bullets_and_tasks() {
    assert_eq!(run("- one|", EditRequest::Newline), "- one\n- |");
    assert_eq!(run("* one|", EditRequest::Newline), "* one\n* |");
    assert_eq!(run("  + one|", EditRequest::Newline), "  + one\n  + |");
    assert_eq!(
        run("- [x] done|", EditRequest::Newline),
        "- [x] done\n- [ ] |"
    );
    assert_eq!(
        run("> - quoted|", EditRequest::Newline),
        "> - quoted\n> - |"
    );
}

#[test]
fn list_moves_the_rest_of_the_line() {
    assert_eq!(run("- one| two", EditRequest::Newline), "- one\n- | two");
}

#[test]
fn list_numbers_ordered_items() {
    assert_eq!(run("1. one|", EditRequest::Newline), "1. one\n2. |");
    assert_eq!(run("9) nine|", EditRequest::Newline), "9) nine\n10) |");
    assert_eq!(
        run(
            "1. one|\n2. two\n   - sub\n3. three\n\n1. other",
            EditRequest::Newline
        ),
        "1. one\n2. |\n3. two\n   - sub\n4. three\n\n1. other"
    );
    assert_eq!(
        run("1. one|\n1. lazy", EditRequest::Newline),
        "1. one\n2. |\n1. lazy"
    );
}

#[test]
fn enter_on_empty_item_ends_the_list() {
    assert_eq!(run("- a\n- |", EditRequest::Newline), "- a\n|");
    assert_eq!(run("- a\n- [ ] |", EditRequest::Newline), "- a\n|");
    assert_eq!(
        run("1. a\n2.  |\nnext", EditRequest::Newline),
        "1. a\n|\nnext"
    );
}

#[test]
fn list_leaves_other_enters_alone() {
    assert_eq!(run("plain|", EditRequest::Newline), "plain\n|");
    assert_eq!(run("|- item", EditRequest::Newline), "\n|- item");
    assert_eq!(run("- a|\n- b|", EditRequest::Newline), "- a\n|\n- b\n|");
    let pipeline = Pipeline::builtin();
    let in_code = run_in(&pipeline, "- a|", EditRequest::Newline, InputContext::Code);
    assert_eq!(in_code, "- a\n|");
}

#[test]
fn auto_pair_inserts_closers() {
    assert_eq!(run("a |", typed("(")), "a (|)");
    assert_eq!(run("|)", typed("[")), "[|])");
    assert_eq!(run("say |", typed("\"")), "say \"|\"");
    assert_eq!(run("|", typed("$")), "$|$");
}

#[test]
fn auto_pair_skips_where_it_would_be_wrong() {
    assert_eq!(run("don|", typed("'")), "don'|");
    assert_eq!(run("5|", typed("$")), "5$|");
    assert_eq!(run("|word", typed("(")), "(|word");
    let pipeline = Pipeline::builtin();
    let in_math = run_in(&pipeline, "x |", typed("$"), InputContext::Math);
    assert_eq!(in_math, "x $|");
    let in_math_bracket = run_in(&pipeline, "x |", typed("("), InputContext::Math);
    assert_eq!(in_math_bracket, "x (|)");
    let in_code = run_in(&pipeline, "|", typed("("), InputContext::Code);
    assert_eq!(in_code, "(|");
}

#[test]
fn auto_pair_overtypes_closers() {
    assert_eq!(run("(a|)", typed(")")), "(a)|");
    assert_eq!(run("$x|$", typed("$")), "$x$|");
    assert_eq!(run("a|b", typed(")")), "a)|b");
}

#[test]
fn auto_pair_wraps_selection() {
    assert_eq!(run("«word»", typed("(")), "(«word»)");
    assert_eq!(run("a «b» c", typed("$")), "a $«b»$ c");
    let (doc, _) = parse("word");
    let backwards = Selection::single(SelectionRange::new(4, 0));
    let tr = Pipeline::builtin()
        .run(typed("["), &doc, &backwards, &FixedContext::default(), 0)
        .unwrap();
    assert_eq!(tr.selection.unwrap().primary(), SelectionRange::new(5, 1));
}

#[test]
fn backspace_deletes_empty_pair() {
    assert_eq!(run("(|)", EditRequest::DeleteBackward), "|");
    assert_eq!(run("x(|)y", EditRequest::DeleteBackward), "x|y");
    assert_eq!(run("(a|)", EditRequest::DeleteBackward), "(|)");
    assert_eq!(run("(|) (|)", EditRequest::DeleteBackward), "| |");
    let pipeline = Pipeline::builtin();
    let in_code = run_in(
        &pipeline,
        "(|)",
        EditRequest::DeleteBackward,
        InputContext::Code,
    );
    assert_eq!(in_code, "|)");
}

#[test]
fn auto_pair_works_with_multiple_carets() {
    assert_eq!(run("a | b |", typed("(")), "a (|) b (|)");
}

struct Upper;

impl PipelineStep for Upper {
    fn run(&self, request: EditRequest, _cx: &StepContext<'_>) -> StepOutcome {
        match request {
            EditRequest::InsertText(text) => StepOutcome::Continue(typed(&text.to_uppercase())),
            other => StepOutcome::Continue(other),
        }
    }
}

struct Swallow;

impl PipelineStep for Swallow {
    fn run(&self, _request: EditRequest, _cx: &StepContext<'_>) -> StepOutcome {
        StepOutcome::Cancel
    }
}

struct Stamp;

impl PipelineStep for Stamp {
    fn run(&self, _request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        let changes = ChangeSet::new(vec![TextEdit::insert(0, "!")]).unwrap();
        StepOutcome::Emit(cx.transaction(changes, None))
    }
}

#[test]
fn filled_placeholder_transforms_requests() {
    let mut pipeline = Pipeline::builtin();
    pipeline.replace("replacements", Box::new(Upper)).unwrap();
    assert_eq!(run_in(&pipeline, "|", typed("a"), InputContext::Text), "A|");
    let math = run_in(&pipeline, "|", typed("a"), InputContext::Math);
    assert_eq!(math, "a|", "replacements never run in math");
    pipeline
        .set_contexts("replacements", ContextFilter::any())
        .unwrap();
    assert_eq!(run_in(&pipeline, "|", typed("a"), InputContext::Math), "A|");
}

#[test]
fn disabling_a_step_skips_it() {
    let mut pipeline = Pipeline::builtin();
    pipeline.set_enabled("auto-pair", false).unwrap();
    assert!(!pipeline.slot("auto-pair").unwrap().is_enabled());
    assert_eq!(run_in(&pipeline, "|", typed("("), InputContext::Text), "(|");
    pipeline.set_enabled("apply", false).unwrap();
    let (doc, selection) = parse("|");
    let result = pipeline.run(typed("a"), &doc, &selection, &FixedContext::default(), 0);
    assert!(result.is_none());
}

#[test]
fn a_step_can_emit_or_cancel() {
    let mut pipeline = Pipeline::builtin();
    pipeline.replace("snippets", Box::new(Stamp)).unwrap();
    assert_eq!(
        run_in(&pipeline, "a|", typed("x"), InputContext::Text),
        "!a|"
    );
    pipeline.replace("snippets", Box::new(Swallow)).unwrap();
    assert_eq!(
        run_in(&pipeline, "a|", typed("x"), InputContext::Text),
        "a|"
    );
}

#[test]
fn steps_can_be_added_moved_and_removed() {
    let mut pipeline = Pipeline::builtin();
    let upper = StepSlot::new("upper", Box::new(Upper), ContextFilter::any());
    pipeline.insert_before("apply", upper).unwrap();
    assert_eq!(run_in(&pipeline, "|", typed("b"), InputContext::Text), "B|");
    let stamp = StepSlot::new(
        "stamp",
        Box::new(Stamp),
        ContextFilter::only(&[InputContext::Html]),
    );
    pipeline.insert_after("snippets", stamp).unwrap();
    assert_eq!(pipeline.step_names()[1], "stamp");
    assert_eq!(run_in(&pipeline, "|", typed("b"), InputContext::Html), "!|");
    pipeline.move_after("stamp", "apply").unwrap();
    assert_eq!(pipeline.step_names().last(), Some(&"stamp"));
    pipeline.move_before("stamp", "snippets").unwrap();
    assert_eq!(pipeline.step_names()[0], "stamp");
    let removed = pipeline.remove("stamp").unwrap();
    assert_eq!(removed.name(), "stamp");
    let extra = StepSlot::placeholder("extra", ContextFilter::any());
    pipeline.push(extra).unwrap();
    assert_eq!(pipeline.step_names().last(), Some(&"extra"));
}

#[test]
fn pipeline_edits_report_errors() {
    let mut pipeline = Pipeline::builtin();
    let duplicate = StepSlot::placeholder("apply", ContextFilter::any());
    assert_eq!(
        pipeline.push(duplicate),
        Err(PipelineError::DuplicateStep("apply".into()))
    );
    assert_eq!(
        pipeline.set_enabled("nope", false),
        Err(PipelineError::UnknownStep("nope".into()))
    );
    assert!(pipeline.move_before("apply", "nope").is_err());
    assert_eq!(pipeline.step_names(), step_names::DEFAULT_ORDER);
}

#[test]
fn reorder_needs_every_step_once() {
    let mut pipeline = Pipeline::builtin();
    let mut order = step_names::DEFAULT_ORDER;
    order.swap(4, 5);
    pipeline.reorder(&order).unwrap();
    assert_eq!(pipeline.step_names(), order);
    let before = pipeline.step_names().join(",");
    assert_eq!(
        pipeline.reorder(&["apply"]),
        Err(PipelineError::IncompleteOrder)
    );
    let mut repeated = order;
    repeated[0] = "apply";
    assert_eq!(
        pipeline.reorder(&repeated),
        Err(PipelineError::IncompleteOrder)
    );
    assert_eq!(pipeline.step_names().join(","), before);
}

#[test]
fn typing_a_list_through_the_editor_undoes_in_steps() {
    let pipeline = Pipeline::builtin();
    let mut state = EditorState::new(Document::new());
    let contexts = FixedContext::default();
    let mut clock = 0;
    let mut send = |state: &mut EditorState, request: EditRequest| {
        clock += 50;
        let tr = pipeline
            .run(request, state.doc(), state.selection(), &contexts, clock)
            .unwrap();
        state.apply(tr).unwrap();
    };
    for ch in "- a".chars() {
        send(&mut state, typed(&ch.to_string()));
    }
    send(&mut state, EditRequest::Newline);
    send(&mut state, typed("b"));
    send(&mut state, EditRequest::Newline);
    send(&mut state, EditRequest::Newline);
    assert_eq!(render(state.doc(), state.selection()), "- a\n- b\n|");
    state.undo(10_000);
    assert_eq!(state.doc().to_string(), "");
}
