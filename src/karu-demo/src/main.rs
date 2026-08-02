use karu::{
    App, Arrangement, Color, ColumnOptions, LazyColumnOptions, Modifier, RowOptions, SpacerOptions,
    TextFieldOptions, TextFieldValue, TextOptions, composable, remember_scroll_state,
    remember_text_field_state,
};
use karu_backend_wgpu::Wgpu;
use karu_basic::{Button, ButtonOptions, Surface, SurfaceOptions};

const CANVAS: Color = Color::rgb(0.94, 0.96, 0.98);
const PANEL: Color = Color::rgb(1.0, 1.0, 1.0);
const INPUT: Color = Color::rgb(0.97, 0.98, 1.0);
const INK: Color = Color::rgb(0.10, 0.14, 0.20);
const MUTED: Color = Color::rgb(0.35, 0.40, 0.48);

#[composable]
fn TodoApp() {
    let tasks = remember_mutable_state(|| {
        vec![
            "Review Compose runtime API".to_string(),
            "Implement Foundation modifiers".to_string(),
            "Verify keyed list state".to_string(),
            "Polish the Quad example".to_string(),
        ]
    });
    let draft = remember_text_field_state("");
    let list_scroll = remember_scroll_state(0.0);
    let item_count = tasks.get().len();
    let draft_text = draft.text();
    let can_add = !draft_text.trim().is_empty();

    Surface(
        SurfaceOptions::new().modifier(Modifier::empty().padding(20.0).background(CANVAS)),
        || {
            Column(
                ColumnOptions::new().modifier(Modifier::empty().fill_max_width()),
                || {
                    Text(
                        "Karu Foundation Playground",
                        TextOptions::new()
                            .color(INK)
                            .font_size(24.0)
                            .modifier(Modifier::empty().background(PANEL).padding(4.0)),
                    );
                    Text(
                        format!("{item_count} tasks  |  scroll: {:.0}", list_scroll.value()),
                        TextOptions::new()
                            .color(MUTED)
                            .modifier(Modifier::empty().padding(2.0).background(INPUT)),
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().height(12.0)));

                    BasicTextField(
                        &draft,
                        TextFieldOptions::new().modifier(
                            Modifier::empty()
                                .fill_max_width()
                                .min_size(0.0, 36.0)
                                .padding(8.0)
                                .background(INPUT)
                                .border(1.0, MUTED)
                                .border_radius(4.0)
                                .test_tag("task-draft"),
                        ),
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().height(8.0)));

                    Row(
                        RowOptions::new()
                            .modifier(Modifier::empty().fill_max_width())
                            .horizontal_arrangement(Arrangement::SpaceBetween),
                        || {
                            let add_tasks = tasks.clone();
                            let draft = draft.clone();
                            Button(
                                move || {
                                    let value = draft.text();
                                    let value = value.trim();
                                    if !value.is_empty() {
                                        add_tasks.update(|items| items.push(value.to_string()));
                                        draft.set_value(TextFieldValue::new(""));
                                    }
                                },
                                ButtonOptions::new().enabled(can_add),
                                || Text("Add task", TextOptions::default()),
                            );

                            let reset_tasks = tasks.clone();
                            Button(
                                move || {
                                    reset_tasks.set(vec!["Fresh task list".to_string()]);
                                },
                                ButtonOptions::new().modifier(
                                    Modifier::empty().background(Color::rgb(0.86, 0.90, 0.96)),
                                ),
                                || Text("Reset", TextOptions::default()),
                            );
                        },
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().height(12.0)));

                    LazyColumn(
                        LazyColumnOptions::new()
                            .state(list_scroll.clone())
                            .modifier(
                                Modifier::empty()
                                    .fill_max_width()
                                    .height(280.0)
                                    .padding(8.0)
                                    .background(PANEL)
                                    .border(1.0, Color::rgb(0.78, 0.82, 0.88))
                                    .border_radius(4.0)
                                    .clip(),
                            ),
                        |list| {
                            list.items(
                                tasks.get(),
                                |task| task.clone(),
                                |task| {
                                    Text(
                                        task,
                                        TextOptions::new().modifier(
                                            Modifier::empty()
                                                .fill_max_width()
                                                .padding(6.0)
                                                .content_description("Task item"),
                                        ),
                                    )
                                },
                            );
                        },
                    );
                },
            );
        },
    );
}

fn main() {
    App::builder()
        .with_renderer(Wgpu.default_system_font())
        .title("Karu Foundation Playground")
        .build()
        .run(TodoApp);
}
