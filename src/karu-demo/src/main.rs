#[allow(unused_imports)]
use karu::{
    Alignment, App, Arrangement, Box, BoxOptions, Color, Column, ColumnOptions, CrossAxisAlignment,
    LazyColumn, LazyColumnOptions, Modifier, MutableState, Role, Row, RowOptions, Spacer,
    SpacerOptions, Text, TextFieldOptions, TextFieldValue, TextOptions, composable,
    remember_mutable_state, remember_scroll_state, remember_text_field_state,
};
use karu_backend_quad::Quad;
use karu_basic::{Button, ButtonOptions, Surface, SurfaceOptions};

const BACKGROUND: Color = Color::rgb(0.94, 0.95, 0.97);
const NAVY: Color = Color::rgb(0.08, 0.12, 0.20);
const NAVY_MUTED: Color = Color::rgb(0.52, 0.58, 0.68);
const SURFACE: Color = Color::rgb(1.0, 1.0, 1.0);
const SURFACE_TINT: Color = Color::rgb(0.97, 0.98, 1.0);
const LINE: Color = Color::rgb(0.87, 0.89, 0.93);
const INK: Color = Color::rgb(0.11, 0.14, 0.20);
const MUTED: Color = Color::rgb(0.42, 0.46, 0.53);
const ACCENT: Color = Color::rgb(0.16, 0.42, 0.88);
const ACCENT_SOFT: Color = Color::rgb(0.90, 0.94, 1.0);
const SUCCESS: Color = Color::rgb(0.12, 0.62, 0.43);

#[derive(Clone, Copy, PartialEq)]
enum TaskFilter {
    All,
    Active,
    Completed,
}

#[derive(Clone, PartialEq)]
struct Task {
    id: u64,
    title: String,
    done: bool,
}

fn initial_tasks() -> Vec<Task> {
    vec![
        Task {
            id: 1,
            title: "Review the Compose runtime API".to_string(),
            done: true,
        },
        Task {
            id: 2,
            title: "Implement Foundation modifiers".to_string(),
            done: true,
        },
        Task {
            id: 3,
            title: "Verify keyed list state".to_string(),
            done: false,
        },
        Task {
            id: 4,
            title: "Polish the Quad example".to_string(),
            done: false,
        },
    ]
}

#[composable]
fn FilterOption(label: String, count: usize, selected: bool, on_click: impl FnMut() + 'static) {
    let background = if selected {
        ACCENT_SOFT
    } else {
        Color::TRANSPARENT
    };
    let label_color = if selected { ACCENT } else { INK };
    let count_color = if selected { ACCENT } else { MUTED };

    Box(
        BoxOptions::new().modifier(
            Modifier::empty()
                .fill_max_width()
                .min_size(0.0, 40.0)
                .padding(10.0)
                .background(background)
                .border_radius(8.0)
                .clip()
                .clickable(on_click)
                .content_description("Filter tasks"),
        ),
        || {
            Row(
                RowOptions::new()
                    .modifier(Modifier::empty().fill_max_width())
                    .horizontal_arrangement(Arrangement::SpaceBetween)
                    .vertical_alignment(CrossAxisAlignment::Center),
                || {
                    Text(label, TextOptions::new().color(label_color).font_size(14.0));
                    Text(
                        count.to_string(),
                        TextOptions::new().color(count_color).font_size(13.0),
                    );
                },
            );
        },
    );
}

#[composable]
fn TaskCard(task: Task, tasks: MutableState<Vec<Task>>) {
    let task_id = task.id;
    let completed = task.done;
    let checkbox_background = if completed { SUCCESS } else { SURFACE };
    let checkbox_text = if completed { "✓" } else { "" };
    let title_color = if completed { MUTED } else { INK };
    let status = if completed {
        "Completed"
    } else {
        "In progress"
    };
    let status_color = if completed { SUCCESS } else { ACCENT };

    Column(
        ColumnOptions::new().modifier(Modifier::empty().fill_max_width()),
        || {
            Row(
                RowOptions::new()
                    .modifier(
                        Modifier::empty()
                            .fill_max_width()
                            .min_size(0.0, 74.0)
                            .padding(14.0)
                            .background(SURFACE)
                            .border(1.0, LINE)
                            .border_radius(10.0)
                            .clip(),
                    )
                    .vertical_alignment(CrossAxisAlignment::Center),
                || {
                    let toggle_tasks = tasks.clone();
                    Box(
                        BoxOptions::new()
                            .content_alignment(Alignment::Center)
                            .modifier(
                                Modifier::empty()
                                    .size(28.0, 28.0)
                                    .background(checkbox_background)
                                    .border(1.0, if completed { SUCCESS } else { LINE })
                                    .border_radius(8.0)
                                    .clip()
                                    .role(Role::Checkbox)
                                    .clickable(move || {
                                        toggle_tasks.update(|items| {
                                            if let Some(item) =
                                                items.iter_mut().find(|item| item.id == task_id)
                                            {
                                                item.done = !item.done;
                                            }
                                        });
                                    }),
                            ),
                        || {
                            Text(
                                checkbox_text,
                                TextOptions::new().color(Color::WHITE).font_size(16.0),
                            );
                        },
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().width(14.0)));

                    Column(
                        ColumnOptions::new().modifier(Modifier::empty().weight(1.0)),
                        || {
                            Text(
                                task.title,
                                TextOptions::new().color(title_color).font_size(15.0),
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(5.0)));
                            Text(
                                status,
                                TextOptions::new().color(status_color).font_size(12.0),
                            );
                        },
                    );

                    let delete_tasks = tasks.clone();
                    Box(
                        BoxOptions::new()
                            .content_alignment(Alignment::Center)
                            .modifier(
                                Modifier::empty()
                                    .size(30.0, 30.0)
                                    .border_radius(8.0)
                                    .clip()
                                    .clickable(move || {
                                        delete_tasks.update(|items| {
                                            items.retain(|item| item.id != task_id);
                                        });
                                    })
                                    .content_description("Delete task"),
                            ),
                        || {
                            Text("×", TextOptions::new().color(MUTED).font_size(20.0));
                        },
                    );
                },
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(10.0)));
        },
    );
}

#[composable]
fn TodoApp() {
    let tasks = remember_mutable_state(initial_tasks);
    let next_id = remember_mutable_state(|| 5_u64);
    let draft = remember_text_field_state("");
    let filter = remember_mutable_state(|| TaskFilter::All);
    let list_scroll = remember_scroll_state(0.0);

    let all_tasks = tasks.get();
    let completed_count = all_tasks.iter().filter(|task| task.done).count();
    let active_count = all_tasks.len().saturating_sub(completed_count);
    let total_count = all_tasks.len();
    let filter_value = filter.get();
    let visible_tasks = all_tasks
        .iter()
        .filter(|task| match filter_value {
            TaskFilter::All => true,
            TaskFilter::Active => !task.done,
            TaskFilter::Completed => task.done,
        })
        .cloned()
        .collect::<Vec<_>>();
    let draft_text = draft.text();
    let can_add = !draft_text.trim().is_empty();

    Surface(
        SurfaceOptions::new().modifier(
            Modifier::empty()
                .fill_max_size()
                .padding(18.0)
                .background(BACKGROUND),
        ),
        || {
            Column(
                ColumnOptions::new().modifier(Modifier::empty().fill_max_size()),
                || {
                    Row(
                        RowOptions::new()
                            .modifier(
                                Modifier::empty()
                                    .fill_max_width()
                                    .min_size(0.0, 78.0)
                                    .padding(20.0)
                                    .background(NAVY)
                                    .border_radius(12.0)
                                    .clip(),
                            )
                            .horizontal_arrangement(Arrangement::SpaceBetween)
                            .vertical_alignment(CrossAxisAlignment::Center),
                        || {
                            Column(ColumnOptions::new(), || {
                                Text(
                                    "KARU / TODO",
                                    TextOptions::new().color(Color::WHITE).font_size(19.0),
                                );
                                Spacer(
                                    SpacerOptions::new().modifier(Modifier::empty().height(4.0)),
                                );
                                Text(
                                    "A calmer place for your next action",
                                    TextOptions::new().color(NAVY_MUTED).font_size(12.0),
                                );
                            });
                            Text(
                                format!("{active_count} active  /  {total_count} total"),
                                TextOptions::new().color(NAVY_MUTED).font_size(13.0),
                            );
                        },
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().height(16.0)));

                    Row(
                        RowOptions::new()
                            .modifier(Modifier::empty().fill_max_width().weight(1.0))
                            .vertical_alignment(CrossAxisAlignment::Start),
                        || {
                            Column(
                                ColumnOptions::new().modifier(
                                    Modifier::empty()
                                        .width(218.0)
                                        .fill_max_height()
                                        .padding(18.0)
                                        .background(SURFACE)
                                        .border(1.0, LINE)
                                        .border_radius(10.0)
                                        .clip(),
                                ),
                                || {
                                    Text(
                                        "WORKSPACE",
                                        TextOptions::new().color(MUTED).font_size(11.0),
                                    );
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(14.0)),
                                    );

                                    let all_filter = filter.clone();
                                    FilterOption(
                                        "All tasks".to_string(),
                                        total_count,
                                        filter_value == TaskFilter::All,
                                        move || all_filter.set(TaskFilter::All),
                                    );
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(6.0)),
                                    );
                                    let active_filter = filter.clone();
                                    FilterOption(
                                        "In progress".to_string(),
                                        active_count,
                                        filter_value == TaskFilter::Active,
                                        move || active_filter.set(TaskFilter::Active),
                                    );
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(6.0)),
                                    );
                                    let completed_filter = filter.clone();
                                    FilterOption(
                                        "Completed".to_string(),
                                        completed_count,
                                        filter_value == TaskFilter::Completed,
                                        move || completed_filter.set(TaskFilter::Completed),
                                    );

                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(28.0)),
                                    );
                                    Text("TODAY", TextOptions::new().color(MUTED).font_size(11.0));
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(12.0)),
                                    );
                                    Text(
                                        format!("{completed_count} of {total_count} tasks done"),
                                        TextOptions::new().color(INK).font_size(13.0),
                                    );
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(9.0)),
                                    );
                                    Box(
                                        BoxOptions::new().modifier(
                                            Modifier::empty()
                                                .fill_max_width()
                                                .height(6.0)
                                                .background(LINE)
                                                .border_radius(4.0)
                                                .clip(),
                                        ),
                                        || {
                                            let progress_width = if total_count == 0 {
                                                0.0
                                            } else {
                                                100.0 * completed_count as f32 / total_count as f32
                                            };
                                            Box(
                                                BoxOptions::new().modifier(
                                                    Modifier::empty()
                                                        .width(progress_width)
                                                        .fill_max_height()
                                                        .background(SUCCESS)
                                                        .border_radius(4.0)
                                                        .clip(),
                                                ),
                                                || {},
                                            );
                                        },
                                    );
                                },
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().width(18.0)));

                            Column(
                                ColumnOptions::new()
                                    .modifier(Modifier::empty().weight(1.0).fill_max_height()),
                                || {
                                    Row(
                                        RowOptions::new()
                                            .modifier(Modifier::empty().fill_max_width())
                                            .horizontal_arrangement(Arrangement::SpaceBetween)
                                            .vertical_alignment(CrossAxisAlignment::Center),
                                        || {
                                            Column(ColumnOptions::new(), || {
                                                Text(
                                                    "Your tasks",
                                                    TextOptions::new().color(INK).font_size(25.0),
                                                );
                                                Spacer(
                                                    SpacerOptions::new()
                                                        .modifier(Modifier::empty().height(5.0)),
                                                );
                                                Text(
                                                    "Keep the list small, clear, and moving.",
                                                    TextOptions::new().color(MUTED).font_size(13.0),
                                                );
                                            });
                                            Text(
                                                format!("{completed_count} completed"),
                                                TextOptions::new()
                                                    .color(SUCCESS)
                                                    .font_size(13.0)
                                                    .modifier(Modifier::empty().padding(8.0)),
                                            );
                                        },
                                    );
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(18.0)),
                                    );

                                    Row(
                                        RowOptions::new()
                                            .modifier(Modifier::empty().fill_max_width())
                                            .vertical_alignment(CrossAxisAlignment::Center),
                                        || {
                                            BasicTextField(
                                                &draft,
                                                TextFieldOptions::new().modifier(
                                                    Modifier::empty()
                                                        .weight(1.0)
                                                        .min_size(0.0, 46.0)
                                                        .padding(12.0)
                                                        .background(SURFACE)
                                                        .border(1.0, LINE)
                                                        .border_radius(9.0)
                                                        .test_tag("task-draft"),
                                                ),
                                            );
                                            Spacer(
                                                SpacerOptions::new()
                                                    .modifier(Modifier::empty().width(10.0)),
                                            );
                                            let add_tasks = tasks.clone();
                                            let add_draft = draft.clone();
                                            let add_next_id = next_id.clone();
                                            Button(
                                                move || {
                                                    let title = add_draft.text();
                                                    let title = title.trim();
                                                    if !title.is_empty() {
                                                        let id = add_next_id.get();
                                                        add_tasks.update(|items| {
                                                            items.push(Task {
                                                                id,
                                                                title: title.to_string(),
                                                                done: false,
                                                            });
                                                        });
                                                        add_next_id.set(id + 1);
                                                        add_draft
                                                            .set_value(TextFieldValue::new(""));
                                                    }
                                                },
                                                ButtonOptions::new().enabled(can_add).modifier(
                                                    Modifier::empty()
                                                        .min_size(126.0, 46.0)
                                                        .border_radius(9.0),
                                                ),
                                                || {
                                                    Text(
                                                        "+  Add task",
                                                        TextOptions::new()
                                                            .color(Color::WHITE)
                                                            .font_size(14.0),
                                                    );
                                                },
                                            );
                                        },
                                    );
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(16.0)),
                                    );

                                    if visible_tasks.is_empty() {
                                        Box(
                                            BoxOptions::new()
                                                .content_alignment(Alignment::Center)
                                                .modifier(
                                                    Modifier::empty()
                                                        .fill_max_width()
                                                        .weight(1.0)
                                                        .background(SURFACE_TINT)
                                                        .border(1.0, LINE)
                                                        .border_radius(10.0)
                                                        .clip(),
                                                ),
                                            || {
                                                Column(
                                                    ColumnOptions::new().horizontal_alignment(
                                                        CrossAxisAlignment::Center,
                                                    ),
                                                    || {
                                                        Text(
                                                            "Nothing here yet",
                                                            TextOptions::new()
                                                                .color(INK)
                                                                .font_size(17.0),
                                                        );
                                                        Spacer(SpacerOptions::new().modifier(
                                                            Modifier::empty().height(6.0),
                                                        ));
                                                        Text(
                                                            "Add a task above to get started.",
                                                            TextOptions::new()
                                                                .color(MUTED)
                                                                .font_size(13.0),
                                                        );
                                                    },
                                                );
                                            },
                                        );
                                    } else {
                                        LazyColumn(
                                            LazyColumnOptions::new()
                                                .state(list_scroll.clone())
                                                .modifier(
                                                    Modifier::empty()
                                                        .fill_max_width()
                                                        .weight(1.0)
                                                        .padding(2.0)
                                                        .clip(),
                                                ),
                                            |list| {
                                                list.items(
                                                    visible_tasks.clone(),
                                                    |task| task.id,
                                                    |task| TaskCard(task, tasks.clone()),
                                                );
                                            },
                                        );
                                    }
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().height(12.0)),
                                    );
                                    Row(
                                        RowOptions::new()
                                            .modifier(Modifier::empty().fill_max_width())
                                            .horizontal_arrangement(Arrangement::SpaceBetween)
                                            .vertical_alignment(CrossAxisAlignment::Center),
                                        || {
                                            Text(
                                                "Tip: click the checkbox to mark a task complete.",
                                                TextOptions::new().color(MUTED).font_size(12.0),
                                            );
                                            let reset_tasks = tasks.clone();
                                            let reset_filter = filter.clone();
                                            let reset_next_id = next_id.clone();
                                            Button(
                                                move || {
                                                    reset_tasks.set(initial_tasks());
                                                    reset_filter.set(TaskFilter::All);
                                                    reset_next_id.set(5);
                                                },
                                                ButtonOptions::new().modifier(
                                                    Modifier::empty()
                                                        .background(SURFACE)
                                                        .border(1.0, LINE)
                                                        .border_radius(8.0),
                                                ),
                                                || {
                                                    Text(
                                                        "Reset list",
                                                        TextOptions::new()
                                                            .color(INK)
                                                            .font_size(12.0),
                                                    );
                                                },
                                            );
                                        },
                                    );
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
        .with_renderer(Quad.default_system_font())
        .title("Karu Todo")
        .size(1100, 720)
        .background(BACKGROUND)
        .build()
        .run(TodoApp);
}
