use karu::{
    BoxOptions, Brush, Color, Composition, Element, ElementKind, Modifier, Offset, PointerEvent,
    PointerKind, PointerPhase, TextOptions, composable,
};
use karu_basic::{Button, ButtonOptions};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn button_is_built_from_box_and_modifiers() {
    let clicks = Rc::new(Cell::new(0));
    let mut composition = {
        let clicks = clicks.clone();
        Composition::new(move || {
            let clicks = clicks.clone();
            Button(
                move || clicks.set(clicks.get() + 1),
                ButtonOptions::default(),
                || karu::Text("Click me", TextOptions::default()),
            );
        })
    };

    let result = composition.compose();
    assert!(contains_kind(&result.root, |kind| matches!(
        kind,
        ElementKind::Box
    )));
    assert!(!contains_kind(&result.root, |kind| {
        matches!(kind, ElementKind::Custom(name) if name.name == "Button")
    }));

    let button = find_layout_box(&result.render_tree.root).expect("button Box exists");
    assert!(button.interactive);
    assert!(button.background.is_some());
    assert!(button.border.is_some());
    assert_eq!(button.bounds.size.height, 36.0);
}

#[test]
fn button_modifier_overrides_default_background() {
    let mut composition = Composition::new(|| {
        Button(
            || {},
            ButtonOptions::new().modifier(Modifier::empty().background(Brush::Solid(Color::WHITE))),
            || karu::Text("Insert", TextOptions::default()),
        );
    });

    let result = composition.compose();
    let button = find_layout_box(&result.render_tree.root).expect("button Box exists");
    assert_eq!(button.background, Some(Color::WHITE));
}

#[test]
fn disabled_button_exposes_semantics_without_click_handler() {
    let mut composition = Composition::new(|| {
        Button(
            || panic!("disabled button must not invoke its callback"),
            ButtonOptions::new().enabled(false),
            || karu::Text("Unavailable", TextOptions::default()),
        );
    });

    let result = composition.compose();
    let button = find_layout_box(&result.render_tree.root).expect("button Box exists");
    assert!(!button.interactive);
    assert!(button.semantics.disabled);
    assert_eq!(button.semantics.role, Some(karu::Role::Button));
}

#[test]
fn button_click_requires_release_inside_target() {
    let clicks = Rc::new(Cell::new(0));
    let mut composition = {
        let clicks = clicks.clone();
        Composition::new(move || {
            let clicks = clicks.clone();
            Button(
                move || clicks.set(clicks.get() + 1),
                ButtonOptions::default(),
                || karu::Text("Click me", TextOptions::default()),
            );
        })
    };

    let first = composition.compose();
    let button = find_layout_box(&first.render_tree.root).expect("button Box exists");
    let inside = Offset::new(button.bounds.origin.x + 2.0, button.bounds.origin.y + 2.0);
    let outside = Offset::new(500.0, 500.0);

    assert!(composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Down,
        position: inside,
        primary: true,
    }));
    let pressed = composition.recompose();
    let pressed_button = pressed
        .render_tree
        .root
        .find(button.id)
        .expect("button remains stable after press");
    assert!(pressed_button.interaction.pressed);
    assert_eq!(
        pressed_button.background,
        Some(karu::Color::rgb(0.08, 0.27, 0.68))
    );

    composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Move,
        position: outside,
        primary: false,
    });
    composition.recompose();
    composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Up,
        position: outside,
        primary: true,
    });
    composition.recompose();
    assert_eq!(clicks.get(), 0);

    composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Down,
        position: inside,
        primary: true,
    });
    composition.recompose();
    composition.dispatch_pointer_event(PointerEvent {
        kind: PointerKind::Mouse,
        phase: PointerPhase::Up,
        position: inside,
        primary: true,
    });
    composition.recompose();
    assert_eq!(clicks.get(), 1);
}

#[composable]
fn CenteredBox() {
    Box(
        BoxOptions::new().modifier(Modifier::empty().size(100.0, 100.0)),
        || {
            Text(
                "x",
                TextOptions::new().modifier(Modifier::empty().align(karu::Alignment::Center)),
            );
            Text(
                "y",
                TextOptions::new().modifier(Modifier::empty().align(karu::Alignment::BottomEnd)),
            );
        },
    );
}

#[test]
fn box_stacks_and_aligns_children() {
    let mut composition = Composition::new(CenteredBox);
    let result = composition.compose();
    let root = &result.render_tree.root;
    let box_node = root
        .children
        .iter()
        .flat_map(|node| node.children.iter())
        .find(|node| matches!(node.kind, ElementKind::Box))
        .expect("Box exists");

    assert_eq!(box_node.children.len(), 2);
    assert_eq!(box_node.children[0].bounds.origin, Offset::new(46.0, 40.0));
    assert_eq!(box_node.children[1].bounds.origin, Offset::new(92.0, 80.0));
}

fn contains_kind(element: &Element, predicate: impl Fn(&ElementKind) -> bool + Copy) -> bool {
    predicate(&element.kind)
        || element
            .children
            .iter()
            .any(|child| contains_kind(child, predicate))
}

fn find_layout_box(root: &karu::LayoutNode) -> Option<&karu::LayoutNode> {
    if matches!(root.kind, ElementKind::Box) {
        return Some(root);
    }
    root.children.iter().find_map(find_layout_box)
}
