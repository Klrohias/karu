#[allow(unused_imports)]
use karu::{
    Animatable, App, AppBackend, AppConfig, Applier, Arrangement, BasicTextField, CaretPosition,
    Clipboard, ClipboardError, Color, Column, ColumnOptions, Composition, CompositionLocal,
    Constraints, CrossAxisAlignment, Element, ElementApplier, ElementKind, FocusRequester,
    FocusState, HeadlessBackend, HeadlessTextLayout, KeyCode, KeyEvent, KeyModifiers, LazyColumn,
    LazyColumnOptions, Modifier, MutableState, Offset, PointerEvent, PointerKind, PointerPhase,
    RecomposeRequest, Recomposer, Rect, RenderCommand, Row, RowOptions, ScrollEvent, ScrollState,
    Size, TaskHandle, TaskRuntime, Text, TextFieldOptions, TextFieldState, TextInputCommand,
    TextInputEvent, TextLayoutEngine, TextOptions, TextWrap, TweenSpec, composable,
    composition_local_of, disposable_effect, key, mutable_state_of, provide,
    remember_mutable_state, side_effect,
};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn text_layout_hits_measured_grapheme_intervals() {
    let mut renderer = HeadlessTextLayout;

    assert_eq!(
        renderer.hit_test_text(
            "a你b",
            14.0,
            f32::INFINITY,
            TextWrap::NoWrap,
            Offset::new(3.0, 4.0)
        ),
        0
    );
    assert_eq!(
        renderer.hit_test_text(
            "a你b",
            14.0,
            f32::INFINITY,
            TextWrap::NoWrap,
            Offset::new(12.0, 4.0)
        ),
        1
    );
    assert_eq!(
        renderer.hit_test_text(
            "a你b",
            14.0,
            f32::INFINITY,
            TextWrap::NoWrap,
            Offset::new(21.0, 4.0)
        ),
        4
    );
    assert_eq!(
        renderer
            .caret_position("a你b", 14.0, f32::INFINITY, TextWrap::NoWrap, 4)
            .position
            .x,
        22.0
    );
}

struct CountingTextLayout {
    calls: Rc<std::cell::Cell<usize>>,
    layout: HeadlessTextLayout,
}

impl TextLayoutEngine for CountingTextLayout {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: f32, wrap: TextWrap) -> Size {
        self.calls.set(self.calls.get() + 1);
        self.layout.measure_text(text, font_size, max_width, wrap)
    }
    fn caret_position(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> CaretPosition {
        self.calls.set(self.calls.get() + 1);
        self.layout
            .caret_position(text, font_size, max_width, wrap, offset)
    }
    fn hit_test_text(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        position: Offset,
    ) -> usize {
        self.calls.set(self.calls.get() + 1);
        self.layout
            .hit_test_text(text, font_size, max_width, wrap, position)
    }
    fn text_line_range(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        offset: usize,
    ) -> std::ops::Range<usize> {
        self.calls.set(self.calls.get() + 1);
        self.layout
            .text_line_range(text, font_size, max_width, wrap, offset)
    }
    fn selection_rects(
        &mut self,
        text: &str,
        font_size: f32,
        max_width: f32,
        wrap: TextWrap,
        range: std::ops::Range<usize>,
    ) -> Vec<Rect> {
        self.calls.set(self.calls.get() + 1);
        self.layout
            .selection_rects(text, font_size, max_width, wrap, range)
    }
}

#[composable]
fn SharedRendererContent() {
    Text("measured", TextOptions::default());
}

#[test]
fn composition_uses_renderer_text_queries_for_layout_and_commands() {
    let calls = Rc::new(std::cell::Cell::new(0));
    let mut composition = Composition::new(SharedRendererContent);
    let mut layout = CountingTextLayout {
        calls: calls.clone(),
        layout: HeadlessTextLayout,
    };
    let mut backend = HeadlessBackend::default();
    composition.render_with(&mut layout, &mut backend).unwrap();
    let result = composition
        .last_result()
        .expect("composition result exists");
    let command = only_text_command(&result.commands);
    let RenderCommand::DrawText { node: node_id, .. } = command else {
        unreachable!()
    };
    let node = result
        .render_tree
        .root
        .find(*node_id)
        .expect("text node exists");

    assert!(calls.get() >= 2);
    assert!(node.text_field.is_none());
}

#[test]
fn public_ui_functions_build_nodes_without_macro_rewriting() {
    let mut composition = Composition::new(|| {
        Column(ColumnOptions::default(), || {
            Text("public", TextOptions::default());
        });
    });

    let result = composition.compose();
    assert!(matches!(result.root.children[0].kind, ElementKind::Column));
    assert!(matches!(
        result.root.children[0].children[0].kind,
        ElementKind::Text(ref value) if value == "public"
    ));
}

#[derive(Default)]
struct MemoryClipboard {
    text: Option<String>,
}

impl Clipboard for MemoryClipboard {
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(self.text.clone())
    }

    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.text = Some(text.to_string());
        Ok(())
    }
}

#[test]
fn clipboard_is_a_backend_capability_with_a_testable_contract() {
    let mut clipboard = MemoryClipboard::default();
    assert_eq!(clipboard.get_text().unwrap(), None);
    clipboard.set_text("copied").unwrap();
    assert_eq!(clipboard.get_text().unwrap().as_deref(), Some("copied"));
}

#[test]
fn element_applier_owns_tree_mutation() {
    let mut applier = ElementApplier::new(Element::new(
        karu::NodeId(0),
        ElementKind::Root,
        Modifier::empty(),
    ));
    applier.begin_node(Element::new(
        karu::NodeId(1),
        ElementKind::Text("child".to_string()),
        Modifier::empty(),
    ));
    applier.end_node();

    let root = applier.finish().expect("balanced applier");
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children[0].id, karu::NodeId(1));
}

#[test]
fn recomposer_only_runs_dirty_compositions() {
    let mut composition = Composition::new(|| Text("frame", TextOptions::default()));
    let mut recomposer = Recomposer::new();
    let mut layout = HeadlessTextLayout;

    assert!(
        recomposer
            .recompose_with(&mut composition, &mut layout)
            .is_some()
    );
    assert!(
        recomposer
            .recompose_with(&mut composition, &mut layout)
            .is_none()
    );
}

#[composable]
fn CounterApp(state_out: Rc<RefCell<Option<MutableState<i32>>>>) {
    let count = remember_mutable_state(|| 1);
    *state_out.borrow_mut() = Some(count.clone());
    Text(count.get().to_string(), TextOptions::default());
}

#[test]
fn remembered_mutable_state_survives_explicit_recompose() {
    let state_out = Rc::new(RefCell::new(None));
    let mut composition = {
        let state_out = state_out.clone();
        Composition::new(move || CounterApp(state_out.clone()))
    };

    let first = composition.compose();
    let first_text = only_text_command(&first.commands);
    let first_node = text_node_id(first_text);
    assert_eq!(text_value(first_text), "1");
    assert!(!composition.is_dirty());

    let state = state_out
        .borrow()
        .as_ref()
        .expect("component exposed state")
        .clone();
    state.set(2);
    assert!(composition.is_dirty());

    let second = composition.recompose();
    let second_text = only_text_command(&second.commands);
    assert_eq!(text_value(second_text), "2");
    assert_eq!(text_node_id(second_text), first_node);
    assert!(!composition.is_dirty());
}

#[test]
fn composable_macro_keeps_original_function_body() {
    let state_out = Rc::new(RefCell::new(None));
    let mut composition = {
        let state_out = state_out.clone();
        Composition::new(move || CounterApp(state_out.clone()))
    };
    composition.compose();

    let state = state_out
        .borrow()
        .as_ref()
        .expect("original body exposed state")
        .clone();
    assert_eq!(state.get(), 1);
}

#[test]
fn a_panicking_composition_does_not_leak_its_composer_context() {
    let mut panicking = Composition::new(|| panic!("intentional test panic"));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| panicking.compose()));
    assert!(panic.is_err());

    let mut healthy = Composition::new(HealthyAfterPanic);
    let result = healthy.compose();
    assert_eq!(
        text_value(only_text_command(&result.commands)),
        "still usable"
    );
}

#[test]
fn a_panicking_provider_does_not_leak_its_composition_local() {
    let local = composition_local_of(|| "default".to_string());
    let should_panic = Rc::new(std::cell::Cell::new(true));
    let mut composition = {
        let local = local.clone();
        let should_panic = should_panic.clone();
        Composition::new(move || {
            if should_panic.replace(false) {
                provide(&local, "leaked".to_string(), || {
                    panic!("intentional test panic")
                });
            } else {
                LocalText(local.current());
            }
        })
    };

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| composition.compose()));
    assert!(panic.is_err());
    let result = composition.compose();
    assert_eq!(text_value(only_text_command(&result.commands)), "default");
}

#[composable]
fn HealthyAfterPanic() {
    Text("still usable", TextOptions::default());
}

#[composable]
fn LocalText(value: String) {
    Text(value, TextOptions::default());
}

#[composable]
fn CountingCounterApp(
    state_out: Rc<RefCell<Option<MutableState<i32>>>>,
    render_count: Rc<RefCell<usize>>,
) {
    *render_count.borrow_mut() += 1;

    let count = remember_mutable_state(|| 1);
    *state_out.borrow_mut() = Some(count.clone());
    Text(count.get().to_string(), TextOptions::default());
}

#[test]
fn state_updates_enqueue_recompose_requests_without_running_root() {
    let state_out = Rc::new(RefCell::new(None));
    let render_count = Rc::new(RefCell::new(0));
    let callbacks = Rc::new(RefCell::new(Vec::<RecomposeRequest>::new()));
    let mut composition = {
        let state_out = state_out.clone();
        let render_count = render_count.clone();
        Composition::new(move || CountingCounterApp(state_out.clone(), render_count.clone()))
    };

    composition.set_recompose_callback({
        let callbacks = callbacks.clone();
        move |request| callbacks.borrow_mut().push(request)
    });

    composition.compose();
    assert_eq!(*render_count.borrow(), 1);
    assert!(composition.take_recompose_requests().is_empty());

    let state = state_out
        .borrow()
        .as_ref()
        .expect("component exposed state")
        .clone();
    state.update(|count| *count = 2);

    assert!(composition.is_dirty());
    assert_eq!(*render_count.borrow(), 1);

    let requests = composition.take_recompose_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(*callbacks.borrow(), requests);
    assert!(composition.is_dirty());

    let second = composition.recompose();
    let second_text = only_text_command(&second.commands);
    assert_eq!(text_value(second_text), "2");
    assert_eq!(*render_count.borrow(), 2);
    assert!(!composition.is_dirty());
}

#[composable]
fn StateReader(state: MutableState<i32>) {
    Text(state.get().to_string(), TextOptions::default());
}

#[composable]
fn StateOwner(out: Rc<RefCell<Option<MutableState<i32>>>>) {
    let state = remember_mutable_state(|| 0);
    *out.borrow_mut() = Some(state.clone());
    StateReader(state);
}

#[test]
fn state_invalidation_targets_the_group_that_reads_it() {
    let out = Rc::new(RefCell::new(None));
    let mut composition = {
        let out = out.clone();
        Composition::new(move || StateOwner(out.clone()))
    };
    composition.compose();
    let state = out.borrow().as_ref().unwrap().clone();
    state.set(1);
    let requests = composition.take_recompose_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].scope.0.len(), 2);
}

#[composable]
fn ConditionalStateReader(
    visible: MutableState<bool>,
    child: Rc<RefCell<Option<MutableState<i32>>>>,
) {
    if visible.get() {
        ConditionalStateChild(child);
    }
}

#[composable]
fn ConditionalStateChild(child: Rc<RefCell<Option<MutableState<i32>>>>) {
    let value = remember_mutable_state(|| 0);
    *child.borrow_mut() = Some(value.clone());
    Text(value.get().to_string(), TextOptions::default());
}

#[test]
fn state_updates_do_not_schedule_removed_recompose_scopes() {
    let visible = mutable_state_of(true);
    let child = Rc::new(RefCell::new(None));
    let mut composition = {
        let visible = visible.clone();
        let child = child.clone();
        Composition::new(move || ConditionalStateReader(visible.clone(), child.clone()))
    };
    composition.compose();
    let state = child.borrow().as_ref().unwrap().clone();

    visible.set(false);
    composition.recompose();
    composition.take_recompose_requests();
    state.set(1);

    assert!(composition.take_recompose_requests().is_empty());
}

#[composable]
fn TodoItem(todo: String) {
    Text(todo, TextOptions::default());
}

#[composable]
fn TodoApp(state_out: Rc<RefCell<Option<MutableState<Vec<String>>>>>) {
    let todos = remember_mutable_state(|| vec!["alpha".to_string(), "beta".to_string()]);
    *state_out.borrow_mut() = Some(todos.clone());

    Column(ColumnOptions::default(), || {
        for todo in todos.get() {
            TodoItem(todo);
        }
    });
}

#[test]
fn list_items_reuse_nodes_by_position() {
    let state_out = Rc::new(RefCell::new(None));
    let mut composition = {
        let state_out = state_out.clone();
        Composition::new(move || TodoApp(state_out.clone()))
    };

    let first = composition.compose();
    let first_texts = text_commands(&first.commands);
    assert_eq!(text_values(&first_texts), vec!["alpha", "beta"]);
    let first_ids = text_ids(&first_texts);

    state_out
        .borrow()
        .as_ref()
        .expect("component exposed todo state")
        .update(|todos| todos.push("gamma".to_string()));

    let second = composition.recompose();
    let second_texts = text_commands(&second.commands);
    assert_eq!(text_values(&second_texts), vec!["alpha", "beta", "gamma"]);
    let second_ids = text_ids(&second_texts);

    assert_eq!(second_ids[0], first_ids[0]);
    assert_eq!(second_ids[1], first_ids[1]);
    assert_ne!(second_ids[2], first_ids[0]);
    assert_ne!(second_ids[2], first_ids[1]);
}

#[composable]
fn KeyedItems(
    items: Vec<(u64, String)>,
    states: Rc<RefCell<std::collections::HashMap<u64, MutableState<String>>>>,
) {
    for (id, label) in items {
        let states = states.clone();
        key(id, || {
            let value = remember_mutable_state(|| label);
            states.borrow_mut().insert(id, value.clone());
            Text(value.get(), TextOptions::default());
        });
    }
}

#[test]
fn key_preserves_state_when_items_reorder() {
    let items = Rc::new(RefCell::new(vec![
        (1, "one".to_string()),
        (2, "two".to_string()),
    ]));
    let states = Rc::new(RefCell::new(std::collections::HashMap::new()));
    let mut composition = {
        let items = items.clone();
        let states = states.clone();
        Composition::new(move || KeyedItems(items.borrow().clone(), states.clone()))
    };
    composition.compose();
    states.borrow().get(&1).unwrap().set("changed".to_string());
    *items.borrow_mut() = vec![(2, "two".to_string()), (1, "one".to_string())];

    let result = composition.recompose();
    assert_eq!(
        text_values(&text_commands(&result.commands)),
        vec!["two", "changed"]
    );
}

#[composable]
fn LocalReader(local: CompositionLocal<String>, values: Rc<RefCell<Vec<String>>>) {
    values.borrow_mut().push(local.current());
    provide(&local, "inner".to_string(), || {
        values.borrow_mut().push(local.current())
    });
    values.borrow_mut().push(local.current());
}

#[test]
fn composition_local_is_scoped_and_nested() {
    let local = composition_local_of(|| "default".to_string());
    let values = Rc::new(RefCell::new(Vec::new()));
    let mut composition = {
        let local = local.clone();
        let values = values.clone();
        Composition::new(move || {
            provide(&local, "outer".to_string(), || {
                LocalReader(local.clone(), values.clone())
            })
        })
    };
    composition.compose();
    assert_eq!(&*values.borrow(), &["outer", "inner", "outer"]);
}

#[composable]
fn EffectContent(show: bool, events: Rc<RefCell<Vec<&'static str>>>) {
    side_effect({
        let events = events.clone();
        move || events.borrow_mut().push("side")
    });
    if show {
        disposable_effect("visible", move || {
            events.borrow_mut().push("start");
            move || events.borrow_mut().push("dispose")
        });
    }
}

#[test]
fn effects_follow_composition_lifecycle() {
    let show = Rc::new(RefCell::new(true));
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut composition = {
        let show = show.clone();
        let events = events.clone();
        Composition::new(move || EffectContent(*show.borrow(), events.clone()))
    };
    composition.compose();
    *show.borrow_mut() = false;
    composition.recompose();
    assert_eq!(&*events.borrow(), &["start", "side", "dispose", "side"]);
}

struct TestTaskHandle;

impl TaskHandle for TestTaskHandle {
    fn cancel(&self) {}
}

struct CountingTaskRuntime(Rc<std::cell::Cell<usize>>);

impl TaskRuntime for CountingTaskRuntime {
    fn spawn(
        &self,
        _: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>>,
    ) -> Rc<dyn TaskHandle> {
        self.0.set(self.0.get() + 1);
        Rc::new(TestTaskHandle)
    }
}

#[composable]
fn LaunchThenPanic() {
    launched_effect("task", std::future::ready(()));
    panic!("intentional test panic");
}

#[composable]
fn LaunchTask() {
    launched_effect("task", std::future::ready(()));
}

#[test]
fn launched_effects_do_not_start_when_composition_panics() {
    let spawns = Rc::new(std::cell::Cell::new(0));
    let runtime = Rc::new(CountingTaskRuntime(spawns.clone()));
    let mut composition = Composition::new(LaunchThenPanic).with_task_runtime(runtime);

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| composition.compose()));
    assert!(panic.is_err());
    assert_eq!(spawns.get(), 0);
}

#[test]
fn launched_effects_start_once_after_successful_composition() {
    let spawns = Rc::new(std::cell::Cell::new(0));
    let runtime = Rc::new(CountingTaskRuntime(spawns.clone()));
    let mut composition = Composition::new(LaunchTask).with_task_runtime(runtime);

    composition.compose();
    composition.recompose();
    assert_eq!(spawns.get(), 1);
}

#[composable]
fn DisposableThenPanic(events: Rc<RefCell<Vec<&'static str>>>) {
    disposable_effect("resource", move || {
        events.borrow_mut().push("setup");
        || {}
    });
    panic!("intentional test panic");
}

#[test]
fn disposable_effects_do_not_setup_when_composition_panics() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut composition = {
        let events = events.clone();
        Composition::new(move || DisposableThenPanic(events.clone()))
    };

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| composition.compose()));
    assert!(panic.is_err());
    assert!(events.borrow().is_empty());
}

#[test]
fn foundation_state_objects_clamp_and_edit() {
    let scroll = ScrollState::new(4.0);
    scroll.set_max_value(10.0);
    assert_eq!(scroll.scroll_by(20.0), 6.0);
    assert_eq!(scroll.value(), 10.0);

    let field = TextFieldState::new("a");
    field.edit(|value| {
        value.text.push('b');
        value.selection = 2..2;
    });
    assert_eq!(field.text(), "ab");
    assert_eq!(field.value().selection, 2..2);
}

#[test]
fn text_field_normalizes_non_boundary_selections_before_editing() {
    let state = TextFieldState::new("a你b");
    state.set_value(karu::TextFieldValue {
        text: "a你b".to_string(),
        selection: 2..3,
        composition: None,
    });

    state.replace_selection("x");
    assert_eq!(state.text(), "ax你b");
}

#[test]
fn text_field_handles_cursor_selection_shortcuts_and_history() {
    let state = TextFieldState::new("one two");
    state.set_cursor(state.text().len());

    let word_select = state.handle_key(KeyEvent {
        code: KeyCode::Left,
        modifiers: KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert!(word_select.handled);
    assert_eq!(state.value().selection, 4..4);

    state.handle_key(KeyEvent {
        code: KeyCode::Right,
        modifiers: KeyModifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(state.selected_text().as_deref(), Some("t"));

    state.handle_key(KeyEvent {
        code: KeyCode::A,
        modifiers: KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
        repeat: false,
    });
    let copied = state.handle_key(KeyEvent {
        code: KeyCode::C,
        modifiers: KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(
        copied.commands,
        vec![TextInputCommand::Copy("one two".to_string())]
    );

    let cut = state.handle_key(KeyEvent {
        code: KeyCode::X,
        modifiers: KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(
        cut.commands,
        vec![TextInputCommand::Cut("one two".to_string())]
    );
    assert_eq!(state.text(), "");
    state.handle_key(KeyEvent {
        code: KeyCode::Z,
        modifiers: KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(state.text(), "one two");
    state.handle_key(KeyEvent {
        code: KeyCode::Z,
        modifiers: KeyModifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(state.text(), "");

    let paste = state.handle_key(KeyEvent {
        code: KeyCode::V,
        modifiers: KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(paste.commands, vec![TextInputCommand::PasteRequest]);
}

#[test]
fn text_field_composition_commits_and_can_be_undone() {
    let state = TextFieldState::new("");
    state.start_composition();
    state.update_composition("ni");
    assert_eq!(state.text(), "ni");
    assert_eq!(state.value().composition, Some(0..2));

    state.commit_composition("你");
    assert_eq!(state.text(), "你");
    assert_eq!(state.value().composition, None);
    state.undo();
    assert_eq!(state.text(), "");

    state.start_composition();
    state.update_composition("x");
    state.end_composition();
    assert_eq!(state.text(), "");
}

#[test]
fn text_field_moves_and_deletes_by_grapheme_cluster() {
    let state = TextFieldState::new("a😀e\u{301}");
    state.set_cursor(state.text().len());
    state.handle_key(KeyEvent {
        code: KeyCode::Left,
        modifiers: KeyModifiers::default(),
        repeat: false,
    });
    assert_eq!(state.value().selection, 5..5);
    state.backspace();
    assert_eq!(state.text(), "ae\u{301}");
    state.handle_key(KeyEvent {
        code: KeyCode::Left,
        modifiers: KeyModifiers::default(),
        repeat: false,
    });
    assert_eq!(state.value().selection, 0..0);
    state.handle_key(KeyEvent {
        code: KeyCode::Right,
        modifiers: KeyModifiers::default(),
        repeat: false,
    });
    assert_eq!(state.value().selection, 1..1);
}

#[test]
fn text_field_keeps_active_endpoint_for_reverse_selection() {
    let state = TextFieldState::new("abcd");
    state.select_range(1..4, Some(4));
    assert_eq!(state.active_endpoint(), 1);
    state.handle_key(KeyEvent {
        code: KeyCode::Left,
        modifiers: KeyModifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(state.value().selection, 0..4);
    assert_eq!(state.active_endpoint(), 0);
}

#[composable]
fn WeightedAndArrangedLayout() {
    Column(
        ColumnOptions::new()
            .modifier(Modifier::empty().size(100.0, 100.0))
            .vertical_arrangement(Arrangement::Center)
            .horizontal_alignment(CrossAxisAlignment::Center),
        || {
            Text("top", TextOptions::default());
            Text("bottom", TextOptions::default());
        },
    );
    Row(
        RowOptions::new().modifier(Modifier::empty().size(100.0, 20.0)),
        || {
            Text(
                "a",
                TextOptions::new().modifier(Modifier::empty().weight(1.0)),
            );
            Text(
                "b",
                TextOptions::new().modifier(Modifier::empty().weight(1.0)),
            );
        },
    );
}

#[test]
fn row_weight_and_column_arrangement_allocate_parent_space() {
    let result = Composition::new(WeightedAndArrangedLayout).compose();
    let rects = text_commands(&result.commands)
        .iter()
        .map(|command| match command {
            RenderCommand::DrawText { rect, .. } => *rect,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(rects[0].origin.y, 30.0);
    assert_eq!(rects[0].origin.x, 38.0);
    assert_eq!(rects[1].origin.y, 50.0);
    assert_eq!(rects[2].size.width, 50.0);
    assert_eq!(rects[3].origin.x, 50.0);
}

#[composable]
fn ScrollContent(state: ScrollState) {
    Column(
        ColumnOptions::new().modifier(Modifier::empty().size(80.0, 40.0).vertical_scroll(state)),
        || {
            Text("one", TextOptions::default());
            Text("two", TextOptions::default());
            Text("three", TextOptions::default());
        },
    );
}

#[test]
fn scroll_modifier_measures_content_and_translates_children() {
    let state = ScrollState::default();
    let mut composition = {
        let state = state.clone();
        Composition::new(move || ScrollContent(state.clone()))
    };
    composition.compose();
    assert_eq!(state.max_value(), 20.0);
    assert!(composition.dispatch_scroll_event(ScrollEvent {
        position: Offset::new(2.0, 2.0),
        delta: Offset::new(0.0, 20.0),
    }));
    let result = composition.recompose();
    let rects = text_commands(&result.commands)
        .iter()
        .map(|command| match command {
            RenderCommand::DrawText { rect, .. } => *rect,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(rects[0].origin.y, -20.0);
    assert_eq!(rects[2].origin.y, 20.0);
}

#[composable]
fn ScrollWithInteractiveChild(state: ScrollState) {
    Column(
        ColumnOptions::new().modifier(Modifier::empty().size(80.0, 40.0).vertical_scroll(state)),
        || {
            Text(
                "tap",
                TextOptions::new().modifier(Modifier::empty().clickable(|| {})),
            );
            Text("two", TextOptions::default());
            Text("three", TextOptions::default());
        },
    );
}

#[test]
fn scroll_event_bubbles_past_interactive_child() {
    let state = ScrollState::default();
    let mut composition = {
        let state = state.clone();
        Composition::new(move || ScrollWithInteractiveChild(state.clone()))
    };
    composition.compose();
    assert!(composition.dispatch_scroll_event(ScrollEvent {
        position: Offset::new(1.0, 1.0),
        delta: Offset::new(0.0, 20.0),
    }));
    assert_eq!(state.value(), 20.0);
}

#[composable]
fn SemanticContent() {
    Text(
        "settings",
        TextOptions::new().modifier(
            Modifier::empty()
                .test_tag("settings-action")
                .content_description("Open settings")
                .role(karu::Role::Button),
        ),
    );
}

#[test]
fn semantics_are_queryable_from_layout_tree() {
    let result = Composition::new(SemanticContent).compose();
    let tagged = result
        .render_tree
        .root
        .find_by_test_tag("settings-action")
        .expect("tagged node exists");
    assert_eq!(tagged.semantics.role, Some(karu::Role::Button));
    assert_eq!(
        result
            .render_tree
            .root
            .find_by_content_description("Open settings")
            .map(|node| node.id),
        Some(tagged.id)
    );
}

#[composable]
fn TextFieldContent(state: TextFieldState) {
    BasicTextField(
        &state,
        TextFieldOptions::new().modifier(Modifier::empty().test_tag("editor")),
    );
}

#[test]
fn basic_text_field_exposes_editing_state_and_semantics() {
    let state = TextFieldState::new("draft");
    let mut composition = Composition::new({
        let state = state.clone();
        move || TextFieldContent(state.clone())
    });
    let result = composition.compose();
    assert_eq!(text_value(only_text_command(&result.commands)), "draft");
    let node = result.render_tree.root.find_by_test_tag("editor").unwrap();
    assert_eq!(node.semantics.role, Some(karu::Role::TextField));
    assert!(
        composition.dispatch_text_input_event(TextInputEvent::Insert {
            position: Offset::new(1.0, 1.0),
            text: "!".to_string(),
        })
    );
    assert_eq!(
        text_value(only_text_command(&composition.recompose().commands)),
        "draft!"
    );
    assert!(
        composition.dispatch_text_input_event(TextInputEvent::Backspace {
            position: Offset::new(1.0, 1.0),
        })
    );
    assert_eq!(state.text(), "draft");
}

#[test]
fn basic_text_field_draws_a_cursor_and_supports_mouse_drag_selection() {
    let state = TextFieldState::new("abcd");
    let mut composition = Composition::new({
        let state = state.clone();
        move || TextFieldContent(state.clone())
    });
    let first = composition.compose();
    let node = first.render_tree.root.find_by_test_tag("editor").unwrap();
    let origin = node.bounds.origin;

    composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Down,
        position: Offset::new(origin.x, origin.y + 1.0),
        primary: true,
    });
    let focused = composition.recompose();
    assert!(
        focused
            .commands
            .iter()
            .any(|command| matches!(command, RenderCommand::DrawCursor { .. }))
    );

    composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Move,
        position: Offset::new(origin.x + 16.0, origin.y + 1.0),
        primary: false,
    });
    composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Up,
        position: Offset::new(origin.x + 16.0, origin.y + 1.0),
        primary: true,
    });
    assert_eq!(state.selected_text().as_deref(), Some("ab"));
    let selected = composition.recompose();
    assert!(
        selected
            .commands
            .iter()
            .any(|command| matches!(command, RenderCommand::DrawSelection { .. }))
    );
}

#[composable]
fn TwoTextFields(first: TextFieldState, second: TextFieldState) {
    Column(ColumnOptions::default(), || {
        BasicTextField(
            &first,
            TextFieldOptions::new().modifier(Modifier::empty().test_tag("first")),
        );
        BasicTextField(
            &second,
            TextFieldOptions::new().modifier(Modifier::empty().test_tag("second")),
        );
    });
}

#[test]
fn text_input_prefers_the_active_field_over_pointer_position() {
    let first = TextFieldState::new("one");
    let second = TextFieldState::new("two");
    let mut composition = {
        let first = first.clone();
        let second = second.clone();
        Composition::new(move || TwoTextFields(first.clone(), second.clone()))
    };
    let result = composition.compose();
    let first_node = result.render_tree.root.find_by_test_tag("first").unwrap();
    let second_node = result.render_tree.root.find_by_test_tag("second").unwrap();

    assert!(composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Down,
        position: Offset::new(
            first_node.bounds.origin.x + 1.0,
            first_node.bounds.origin.y + 1.0
        ),
        primary: true,
    }));
    assert!(
        composition.dispatch_text_input_event(TextInputEvent::Insert {
            position: Offset::new(
                second_node.bounds.origin.x + 1.0,
                second_node.bounds.origin.y + 1.0
            ),
            text: "!".to_string(),
        })
    );

    assert_eq!(first.text(), "!one");
    assert_eq!(second.text(), "two");
}

#[composable]
fn FocusableContent(requester: FocusRequester, state: FocusState) {
    Text(
        "focus me",
        TextOptions::new().modifier(Modifier::empty().focusable(requester, state)),
    );
}

#[test]
fn focusable_modifier_requests_focus_and_exposes_semantics() {
    let requester = FocusRequester::new();
    let state = FocusState::default();
    let mut composition = {
        let state = state.clone();
        Composition::new(move || FocusableContent(requester, state.clone()))
    };
    composition.compose();
    assert!(composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Down,
        position: Offset::new(1.0, 1.0),
        primary: true,
    }));
    let result = composition.recompose();
    assert!(state.is_focused(requester));
    assert!(
        result.render_tree.root.children[0].children[0]
            .semantics
            .focused
    );
}

#[test]
fn focus_requesters_have_distinct_runtime_identity() {
    assert_ne!(FocusRequester::new(), FocusRequester::new());
}

#[composable]
fn LazyItems(values: Vec<String>, state: ScrollState) {
    LazyColumn(
        LazyColumnOptions::new()
            .state(state)
            .modifier(Modifier::empty().size(80.0, 40.0)),
        |list| {
            list.items(
                values,
                |value| value.clone(),
                |value| Text(value, TextOptions::default()),
            );
        },
    );
}

#[test]
fn lazy_column_keeps_keyed_items_and_scrolls() {
    let items = Rc::new(RefCell::new(vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
    ]));
    let state = ScrollState::default();
    let mut composition = {
        let items = items.clone();
        let state = state.clone();
        Composition::new(move || LazyItems(items.borrow().clone(), state.clone()))
    };
    let first = composition.compose();
    let ids = text_ids(&text_commands(&first.commands));
    *items.borrow_mut() = vec!["three".to_string(), "one".to_string(), "two".to_string()];
    let second = composition.recompose();
    let second_ids = text_ids(&text_commands(&second.commands));
    assert_eq!(second_ids[0], ids[2]);
    assert_eq!(second_ids[1], ids[0]);
    assert_eq!(state.max_value(), 20.0);
}

#[composable]
fn DefaultLazyItems() {
    LazyColumn(
        LazyColumnOptions::new().modifier(Modifier::empty().size(80.0, 40.0).test_tag("list")),
        |list| {
            for value in ["one", "two", "three"] {
                list.item(value, || Text(value, TextOptions::default()));
            }
        },
    );
}

#[test]
fn lazy_column_remembers_its_default_scroll_state() {
    let mut composition = Composition::new(DefaultLazyItems);
    let first = composition.compose();
    let list = first.render_tree.root.find_by_test_tag("list").unwrap();
    let first_origin = list.children[0].bounds.origin;

    assert!(composition.dispatch_scroll_event(ScrollEvent {
        position: Offset::new(1.0, 1.0),
        delta: Offset::new(0.0, 10.0),
    }));
    let second = composition.recompose();
    let list = second.render_tree.root.find_by_test_tag("list").unwrap();
    assert_eq!(list.children[0].bounds.origin.y, first_origin.y - 10.0);
}

#[test]
fn animatable_advances_with_host_frame_deltas() {
    let animation = Animatable::new(0.0);
    animation.animate_to(10.0, TweenSpec::new(100.0));
    assert!(animation.advance_by(25.0));
    assert_eq!(animation.value(), 2.5);
    assert!(!animation.advance_by(75.0));
    assert_eq!(animation.value(), 10.0);
}

#[composable]
fn StyledLayout() {
    Column(
        ColumnOptions::new().modifier(
            Modifier::empty()
                .padding(2.0)
                .fill_max_width()
                .background(Color::WHITE),
        ),
        || {
            Text(
                "done",
                TextOptions::new().modifier(Modifier::empty().height(10.0)),
            );
        },
    );
}

#[test]
fn modifiers_affect_layout_and_render_commands() {
    let mut composition =
        Composition::new(StyledLayout).with_constraints(Constraints::loose(100.0, 100.0));

    let result = composition.compose();
    let column = &result.render_tree.root.children[0];
    assert_eq!(column.bounds.size.width, 100.0);
    assert_eq!(column.bounds.size.height, 14.0);

    let fill = result
        .commands
        .iter()
        .find_map(|command| match command {
            RenderCommand::FillRect { rect, color, .. } => Some((*rect, *color)),
            _ => None,
        })
        .expect("background modifier emits a fill command");

    assert_eq!(fill.0.size.width, 100.0);
    assert_eq!(fill.0.size.height, 14.0);
    assert_eq!(fill.1, Color::WHITE);

    let text = only_text_command(&result.commands);
    assert_eq!(text_value(text), "done");
}

#[composable]
fn StyledText() {
    Text(
        "styled",
        TextOptions::new()
            .color(Color::rgb(0.2, 0.4, 0.6))
            .font_size(18.0),
    );
}

#[test]
fn text_options_flow_into_render_commands() {
    let result = Composition::new(StyledText).compose();
    let command = only_text_command(&result.commands);
    let RenderCommand::DrawText { style, .. } = command else {
        unreachable!()
    };
    assert_eq!(style.color, Color::rgb(0.2, 0.4, 0.6));
    assert_eq!(style.font_size, 18.0);
}

#[test]
fn composition_tree_records_component_boundaries() {
    let state_out = Rc::new(RefCell::new(None));
    let mut composition = {
        let state_out = state_out.clone();
        Composition::new(move || CounterApp(state_out.clone()))
    };

    let result = composition.compose();
    assert!(matches!(result.root.kind, ElementKind::Root));
    assert!(matches!(
        result.root.children[0].kind,
        ElementKind::Component("CounterApp")
    ));
}

#[derive(Clone, Default)]
struct MockAppBackend {
    state: Rc<RefCell<MockAppState>>,
}

#[derive(Default)]
struct MockAppState {
    config: Option<AppConfig>,
    commands: Vec<RenderCommand>,
}

impl AppBackend for MockAppBackend {
    fn run(self, root: karu::AppRoot, config: AppConfig) {
        let mut composition = Composition::new(root);
        let result = composition.compose();
        let mut state = self.state.borrow_mut();
        state.config = Some(config);
        state.commands = result.commands;
    }
}

#[composable]
fn AppRootComponent() {
    Text("hello app", TextOptions::default());
}

#[test]
fn app_builder_runs_composable_root_with_backend() {
    let state = Rc::new(RefCell::new(MockAppState::default()));

    App::builder()
        .with_renderer(MockAppBackend {
            state: state.clone(),
        })
        .title("Karu Test")
        .size(320, 240)
        .build()
        .run(AppRootComponent);

    let state = state.borrow();
    let config = state.config.as_ref().expect("backend received app config");
    assert_eq!(config.title, "Karu Test");
    assert_eq!(config.width, 320);
    assert_eq!(config.height, 240);
    assert_eq!(text_value(only_text_command(&state.commands)), "hello app");
}

fn only_text_command(commands: &[RenderCommand]) -> &RenderCommand {
    let texts = text_commands(commands);
    assert_eq!(texts.len(), 1);
    texts[0]
}

fn text_commands(commands: &[RenderCommand]) -> Vec<&RenderCommand> {
    commands
        .iter()
        .filter(|command| matches!(command, RenderCommand::DrawText { .. }))
        .collect()
}

fn text_value(command: &RenderCommand) -> &str {
    match command {
        RenderCommand::DrawText { text, .. } => text,
        _ => panic!("expected text command"),
    }
}

fn text_node_id(command: &RenderCommand) -> karu::NodeId {
    match command {
        RenderCommand::DrawText { node, .. } => *node,
        _ => panic!("expected text command"),
    }
}

fn text_values(commands: &[&RenderCommand]) -> Vec<String> {
    commands
        .iter()
        .map(|command| text_value(command).to_string())
        .collect()
}

fn text_ids(commands: &[&RenderCommand]) -> Vec<karu::NodeId> {
    commands
        .iter()
        .map(|command| text_node_id(command))
        .collect()
}
