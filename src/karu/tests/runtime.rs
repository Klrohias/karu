#[allow(unused_imports)]
use karu::{
    App, AppBackend, AppConfig, AppRoot, Color, Column, Column_with_modifier, Composer,
    Composition, Constraints, ElementKind, Modifier, RecomposeRequest, RenderCommand, State, Text,
    Text_with_modifier, composable, mangled_composable, remember_state,
};
use std::cell::RefCell;
use std::rc::Rc;

#[composable]
fn CounterApp(state_out: Rc<RefCell<Option<State<i32>>>>) {
    let count = remember_state(|| 1);
    *state_out.borrow_mut() = Some(count.clone());
    Text(count.get().to_string());
}

#[test]
fn remember_state_survives_explicit_recompose() {
    let state_out = Rc::new(RefCell::new(None));
    let mut composition = {
        let state_out = state_out.clone();
        Composition::new(move |__composer| {
            mangled_composable!(CounterApp)(__composer, state_out.clone())
        })
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
fn composable_macro_keeps_original_function_as_empty_stub() {
    let state_out = Rc::new(RefCell::new(None));
    CounterApp(state_out.clone());

    assert!(state_out.borrow().is_none());
}

#[composable]
fn CountingCounterApp(
    state_out: Rc<RefCell<Option<State<i32>>>>,
    render_count: Rc<RefCell<usize>>,
) {
    *render_count.borrow_mut() += 1;

    let count = remember_state(|| 1);
    *state_out.borrow_mut() = Some(count.clone());
    Text(count.get().to_string());
}

#[test]
fn state_updates_enqueue_recompose_requests_without_running_root() {
    let state_out = Rc::new(RefCell::new(None));
    let render_count = Rc::new(RefCell::new(0));
    let callbacks = Rc::new(RefCell::new(Vec::<RecomposeRequest>::new()));
    let mut composition = {
        let state_out = state_out.clone();
        let render_count = render_count.clone();
        Composition::new(move |__composer| {
            mangled_composable!(CountingCounterApp)(
                __composer,
                state_out.clone(),
                render_count.clone(),
            )
        })
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
fn TodoItem(todo: String) {
    Text(todo);
}

#[composable]
fn TodoApp(state_out: Rc<RefCell<Option<State<Vec<String>>>>>) {
    let todos = remember_state(|| vec!["alpha".to_string(), "beta".to_string()]);
    *state_out.borrow_mut() = Some(todos.clone());

    Column(|| {
        for todo in todos.iter() {
            TodoItem(todo);
        }
    });
}

#[test]
fn list_items_reuse_nodes_by_position() {
    let state_out = Rc::new(RefCell::new(None));
    let mut composition = {
        let state_out = state_out.clone();
        Composition::new(move |__composer| {
            mangled_composable!(TodoApp)(__composer, state_out.clone())
        })
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
fn StyledLayout() {
    Column_with_modifier(
        Modifier::empty()
            .padding(2.0)
            .fill_max_width()
            .background(Color::WHITE),
        || {
            Text_with_modifier("done", Modifier::empty().height(10.0));
        },
    );
}

#[test]
fn modifiers_affect_layout_and_render_commands() {
    let mut composition = Composition::new(mangled_composable!(StyledLayout))
        .with_constraints(Constraints::loose(100.0, 100.0));

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

#[test]
fn composition_tree_records_component_boundaries() {
    let state_out = Rc::new(RefCell::new(None));
    let mut composition = {
        let state_out = state_out.clone();
        Composition::new(move |__composer| {
            mangled_composable!(CounterApp)(__composer, state_out.clone())
        })
    };

    let result = composition.compose();
    assert!(matches!(result.root.kind, ElementKind::Root));
    assert!(matches!(
        result.root.children[0].kind,
        ElementKind::Component("CounterApp")
    ));
}

#[test]
fn runtime_has_no_thread_local_current_composer_path() {
    let composition_source = include_str!("../src/composition.rs");
    assert!(!composition_source.contains("thread_local!"));
    assert!(!composition_source.contains("CURRENT_COMPOSER"));
    assert!(!composition_source.contains("run_in_current_scope"));
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
    fn run(self, mut root: AppRoot, config: AppConfig) {
        let mut composition = Composition::new(move |composer: &mut Composer| root(composer));
        let result = composition.compose();
        let mut state = self.state.borrow_mut();
        state.config = Some(config);
        state.commands = result.commands;
    }
}

#[composable]
fn AppRootComponent() {
    Text("hello app");
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
        .run(mangled_composable!(AppRootComponent));

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
