#[allow(unused_imports)]
use karu::{
    Alignment, App, Arrangement, BasicTextField, Box, BoxOptions, Color, Column, ColumnOptions,
    CrossAxisAlignment, LazyColumn, LazyColumnOptions, Modifier, MutableState, Row, RowOptions,
    ScrollState, Spacer, SpacerOptions, Text, TextFieldOptions, TextFieldState, TextFieldValue,
    TextOptions, composable, key, remember_mutable_state, remember_scroll_state,
    remember_text_field_state,
};
use karu_backend_wgpu::Wgpu;
use karu_basic::{Button, ButtonOptions, Surface, SurfaceOptions};

const CANVAS: Color = Color::rgb(0.95, 0.96, 0.98);
const INK: Color = Color::rgb(0.10, 0.13, 0.19);
const MUTED: Color = Color::rgb(0.43, 0.48, 0.57);
const LINE: Color = Color::rgb(0.86, 0.88, 0.92);
const SURFACE: Color = Color::rgb(1.0, 1.0, 1.0);
const SOFT: Color = Color::rgb(0.97, 0.98, 1.0);
const ACCENT: Color = Color::rgb(0.16, 0.40, 0.82);
const ACCENT_SOFT: Color = Color::rgb(0.88, 0.93, 1.0);
const GREEN: Color = Color::rgb(0.12, 0.60, 0.40);
const GREEN_SOFT: Color = Color::rgb(0.88, 0.96, 0.92);
const AMBER: Color = Color::rgb(0.80, 0.47, 0.12);

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Overview,
    StateLab,
    ModifierLab,
}

#[derive(Clone, Copy, PartialEq)]
enum TaskFilter {
    All,
    Open,
    Done,
}

#[derive(Clone, PartialEq)]
struct Task {
    id: u64,
    title: String,
    detail: String,
    done: bool,
}

fn initial_tasks() -> Vec<Task> {
    vec![
        Task {
            id: 1,
            title: "Trace modifier bounds".to_string(),
            detail: "Compare outer and inner paint layers".to_string(),
            done: true,
        },
        Task {
            id: 2,
            title: "Exercise keyed state".to_string(),
            detail: "Keep list state stable while items move".to_string(),
            done: false,
        },
        Task {
            id: 3,
            title: "Tune text input".to_string(),
            detail: "Check cursor, selection and composition".to_string(),
            done: false,
        },
        Task {
            id: 4,
            title: "Run backend checks".to_string(),
            detail: "Render the same command stream twice".to_string(),
            done: false,
        },
    ]
}

fn panel(extra: Modifier) -> Modifier {
    Modifier::empty()
        .background(SURFACE)
        .border(1.0, LINE)
        .border_radius(10.0)
        .clip()
        .then_modifier(extra)
}

fn soft_panel(extra: Modifier) -> Modifier {
    Modifier::empty()
        .background(SOFT)
        .border(1.0, LINE)
        .border_radius(10.0)
        .clip()
        .then_modifier(extra)
}

#[composable]
fn NavItem(label: String, marker: &'static str, selected: bool, on_click: impl FnMut() + 'static) {
    let background = if selected {
        ACCENT_SOFT
    } else {
        Color::TRANSPARENT
    };
    let color = if selected { ACCENT } else { INK };
    Box(
        BoxOptions::new().modifier(
            Modifier::empty()
                .fill_max_width()
                .min_size(0.0, 42.0)
                .background(background)
                .border_radius(7.0)
                .clip()
                .clickable(on_click)
                .padding(10.0)
                .content_description(label.clone()),
        ),
        || {
            Row(
                RowOptions::new()
                    .modifier(Modifier::empty().fill_max_width())
                    .vertical_alignment(CrossAxisAlignment::Center),
                || {
                    Text(marker, TextOptions::new().color(color).font_size(13.0));
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().width(10.0)));
                    Text(label, TextOptions::new().color(color).font_size(13.0));
                },
            );
        },
    );
}

#[composable]
fn Sidebar(screen: MutableState<Screen>, tasks: &[Task]) {
    let selected = screen.get();
    let done = tasks.iter().filter(|task| task.done).count();
    Column(
        ColumnOptions::new().modifier(panel(
            Modifier::empty()
                .width(220.0)
                .fill_max_height()
                .padding(16.0),
        )),
        || {
            Text("KARU", TextOptions::new().color(INK).font_size(22.0));
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(3.0)));
            Text(
                "RUNTIME CONSOLE",
                TextOptions::new().color(MUTED).font_size(10.0),
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(24.0)));
            Text("PAGES", TextOptions::new().color(MUTED).font_size(10.0));
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(8.0)));

            let overview = screen.clone();
            NavItem(
                "Overview".to_string(),
                "01",
                selected == Screen::Overview,
                move || overview.set(Screen::Overview),
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(5.0)));
            let state_lab = screen.clone();
            NavItem(
                "State Lab".to_string(),
                "02",
                selected == Screen::StateLab,
                move || state_lab.set(Screen::StateLab),
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(5.0)));
            let modifier_lab = screen.clone();
            NavItem(
                "Modifier Lab".to_string(),
                "03",
                selected == Screen::ModifierLab,
                move || modifier_lab.set(Screen::ModifierLab),
            );

            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(24.0)));
            Text("SESSION", TextOptions::new().color(MUTED).font_size(10.0));
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(10.0)));
            Row(
                RowOptions::new().vertical_alignment(CrossAxisAlignment::Center),
                || {
                    Box(
                        BoxOptions::new().modifier(
                            Modifier::empty()
                                .size(8.0, 8.0)
                                .background(GREEN)
                                .border_radius(4.0)
                                .clip(),
                        ),
                        || {},
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().width(8.0)));
                    Text(
                        "All systems ready",
                        TextOptions::new().color(INK).font_size(12.0),
                    );
                },
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(8.0)));
            Text(
                format!("{done} completed checks"),
                TextOptions::new().color(MUTED).font_size(11.0),
            );
        },
    );
}

#[composable]
fn Metric(label: &'static str, value: String, detail: &'static str, tint: Color) {
    Column(
        ColumnOptions::new().modifier(panel(Modifier::empty().weight(1.0).padding(16.0))),
        || {
            Text(label, TextOptions::new().color(MUTED).font_size(10.0));
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(8.0)));
            Text(value, TextOptions::new().color(tint).font_size(25.0));
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(4.0)));
            Text(detail, TextOptions::new().color(MUTED).font_size(11.0));
        },
    );
}

#[composable]
fn FilterButton(label: &'static str, selected: bool, on_click: impl FnMut() + 'static) {
    let background = if selected {
        ACCENT_SOFT
    } else {
        Color::TRANSPARENT
    };
    let color = if selected { ACCENT } else { MUTED };
    Box(
        BoxOptions::new().modifier(
            Modifier::empty()
                .background(background)
                .border_radius(6.0)
                .clip()
                .clickable(on_click)
                .padding(8.0),
        ),
        || Text(label, TextOptions::new().color(color).font_size(11.0)),
    );
}

#[composable]
fn TaskRow(task: Task, tasks: MutableState<Vec<Task>>) {
    let task_id = task.id;
    let title_color = if task.done { MUTED } else { INK };
    let status_color = if task.done { GREEN } else { ACCENT };
    let status = if task.done { "DONE" } else { "OPEN" };
    let toggle_tasks = tasks.clone();
    let delete_tasks = tasks.clone();
    Row(
        RowOptions::new()
            .modifier(panel(Modifier::empty().fill_max_width().padding(13.0)))
            .vertical_alignment(CrossAxisAlignment::Center),
        || {
            Box(
                BoxOptions::new().modifier(
                    Modifier::empty()
                        .size(22.0, 22.0)
                        .background(if task.done { GREEN } else { SURFACE })
                        .border(1.0, if task.done { GREEN } else { LINE })
                        .border_radius(6.0)
                        .clip()
                        .clickable(move || {
                            toggle_tasks.update(|items| {
                                if let Some(item) = items.iter_mut().find(|item| item.id == task_id)
                                {
                                    item.done = !item.done;
                                }
                            });
                        })
                        .content_description("Toggle task"),
                ),
                || {
                    if task.done {
                        Text("OK", TextOptions::new().color(Color::WHITE).font_size(8.0));
                    }
                },
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().width(12.0)));
            Column(
                ColumnOptions::new().modifier(Modifier::empty().weight(1.0)),
                || {
                    Text(
                        task.title,
                        TextOptions::new().color(title_color).font_size(13.0),
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().height(3.0)));
                    Text(task.detail, TextOptions::new().color(MUTED).font_size(11.0));
                },
            );
            Text(
                status,
                TextOptions::new().color(status_color).font_size(10.0),
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().width(10.0)));
            Box(
                BoxOptions::new().modifier(
                    Modifier::empty()
                        .size(24.0, 24.0)
                        .border_radius(5.0)
                        .clip()
                        .clickable(move || {
                            delete_tasks.update(|items| {
                                items.retain(|item| item.id != task_id);
                            });
                        })
                        .content_description("Remove task"),
                ),
                || Text("X", TextOptions::new().color(MUTED).font_size(10.0)),
            );
        },
    );
}

#[composable]
fn Overview(
    tasks: MutableState<Vec<Task>>,
    next_id: MutableState<u64>,
    filter: MutableState<TaskFilter>,
    list_scroll: ScrollState,
) {
    let all = tasks.get();
    let done = all.iter().filter(|task| task.done).count();
    let open = all.len().saturating_sub(done);
    let selected_filter = filter.get();
    let visible = all
        .iter()
        .filter(|task| match selected_filter {
            TaskFilter::All => true,
            TaskFilter::Open => !task.done,
            TaskFilter::Done => task.done,
        })
        .cloned()
        .collect::<Vec<_>>();
    let draft = remember_text_field_state("");
    let can_add = !draft.text().trim().is_empty();

    Column(
        ColumnOptions::new().modifier(Modifier::empty().fill_max_size()),
        || {
            Row(
                RowOptions::new()
                    .modifier(Modifier::empty().fill_max_width())
                    .horizontal_arrangement(Arrangement::SpaceBetween)
                    .vertical_alignment(CrossAxisAlignment::Center),
                || {
                    Column(ColumnOptions::new(), || {
                        Text("Overview", TextOptions::new().color(INK).font_size(25.0));
                        Spacer(SpacerOptions::new().modifier(Modifier::empty().height(4.0)));
                        Text(
                            "A live readout of the current Karu composition.",
                            TextOptions::new().color(MUTED).font_size(12.0),
                        );
                    });
                    Text(
                        "FRAME  /  READY",
                        TextOptions::new().color(GREEN).font_size(11.0),
                    );
                },
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(18.0)));
            Row(
                RowOptions::new()
                    .modifier(Modifier::empty().fill_max_width())
                    .horizontal_arrangement(Arrangement::SpaceBetween),
                || {
                    Metric("OPEN TASKS", open.to_string(), "awaiting attention", ACCENT);
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().width(10.0)));
                    Metric("COMPLETED", done.to_string(), "stable keyed items", GREEN);
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().width(10.0)));
                    Metric(
                        "COMMANDS",
                        (all.len() * 4 + 12).to_string(),
                        "backend-neutral output",
                        AMBER,
                    );
                },
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(14.0)));

            Row(
                RowOptions::new()
                    .modifier(Modifier::empty().fill_max_width().weight(1.0))
                    .vertical_alignment(CrossAxisAlignment::Start),
                || {
                    Column(
                        ColumnOptions::new().modifier(panel(
                            Modifier::empty()
                                .weight(1.0)
                                .fill_max_height()
                                .padding(16.0),
                        )),
                        || {
                            Row(
                                RowOptions::new()
                                    .modifier(Modifier::empty().fill_max_width())
                                    .horizontal_arrangement(Arrangement::SpaceBetween)
                                    .vertical_alignment(CrossAxisAlignment::Center),
                                || {
                                    Column(ColumnOptions::new(), || {
                                        Text(
                                            "Task queue",
                                            TextOptions::new().color(INK).font_size(16.0),
                                        );
                                        Spacer(
                                            SpacerOptions::new()
                                                .modifier(Modifier::empty().height(3.0)),
                                        );
                                        Text(
                                            format!("{} visible records", visible.len()),
                                            TextOptions::new().color(MUTED).font_size(11.0),
                                        );
                                    });
                                    Row(
                                        RowOptions::new()
                                            .modifier(Modifier::empty())
                                            .vertical_alignment(CrossAxisAlignment::Center),
                                        || {
                                            let all_filter = filter.clone();
                                            FilterButton(
                                                "ALL",
                                                selected_filter == TaskFilter::All,
                                                move || all_filter.set(TaskFilter::All),
                                            );
                                            let open_filter = filter.clone();
                                            FilterButton(
                                                "OPEN",
                                                selected_filter == TaskFilter::Open,
                                                move || open_filter.set(TaskFilter::Open),
                                            );
                                            let done_filter = filter.clone();
                                            FilterButton(
                                                "DONE",
                                                selected_filter == TaskFilter::Done,
                                                move || done_filter.set(TaskFilter::Done),
                                            );
                                        },
                                    );
                                },
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(12.0)));
                            Row(
                                RowOptions::new()
                                    .modifier(Modifier::empty().fill_max_width())
                                    .vertical_alignment(CrossAxisAlignment::Center),
                                || {
                                    BasicTextField(
                                        &draft,
                                        TextFieldOptions::new().modifier(
                                            Modifier::empty()
                                                .fill_max_width()
                                                .weight(1.0)
                                                .min_size(0.0, 40.0)
                                                .background(SOFT)
                                                .border(1.0, LINE)
                                                .border_radius(7.0)
                                                .clip()
                                                .padding(10.0),
                                        ),
                                    );
                                    Spacer(
                                        SpacerOptions::new().modifier(Modifier::empty().width(8.0)),
                                    );
                                    let add_tasks = tasks.clone();
                                    let add_next = next_id.clone();
                                    let add_draft = draft.clone();
                                    Button(
                                        move || {
                                            let title = add_draft.text().trim().to_string();
                                            if title.is_empty() {
                                                return;
                                            }
                                            let id = add_next.get();
                                            add_next.set(id + 1);
                                            add_tasks.update(|items| {
                                                items.push(Task {
                                                    id,
                                                    detail: "Added from the live task queue"
                                                        .to_string(),
                                                    title,
                                                    done: false,
                                                });
                                            });
                                            add_draft.set_value(TextFieldValue::new(""));
                                        },
                                        ButtonOptions::new()
                                            .enabled(can_add)
                                            .modifier(Modifier::empty().min_size(66.0, 40.0)),
                                        || {
                                            Text(
                                                "ADD",
                                                TextOptions::new()
                                                    .color(Color::WHITE)
                                                    .font_size(11.0),
                                            )
                                        },
                                    );
                                },
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(12.0)));
                            LazyColumn(
                                LazyColumnOptions::new()
                                    .state(list_scroll.clone())
                                    .item_extent(72.0)
                                    .viewport_height(360.0)
                                    .modifier(
                                        Modifier::empty().fill_max_width().weight(1.0).clip(),
                                    ),
                                |list| {
                                    list.items(
                                        visible,
                                        |task| task.id,
                                        |task| TaskRow(task, tasks.clone()),
                                    );
                                },
                            );
                        },
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().width(14.0)));
                    Column(
                        ColumnOptions::new().modifier(soft_panel(
                            Modifier::empty()
                                .width(190.0)
                                .fill_max_height()
                                .padding(16.0),
                        )),
                        || {
                            Text("RUNTIME", TextOptions::new().color(MUTED).font_size(10.0));
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(12.0)));
                            Text("Composition", TextOptions::new().color(INK).font_size(12.0));
                            Text("stable", TextOptions::new().color(GREEN).font_size(11.0));
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(14.0)));
                            Text("Recomposer", TextOptions::new().color(INK).font_size(12.0));
                            Text("idle", TextOptions::new().color(GREEN).font_size(11.0));
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(14.0)));
                            Text(
                                "Scroll state",
                                TextOptions::new().color(INK).font_size(12.0),
                            );
                            Text(
                                format!("{:.0} px", list_scroll.value()),
                                TextOptions::new().color(ACCENT).font_size(11.0),
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(26.0)));
                            Text(
                                "The queue uses keyed lazy items, mutable state, text input and scroll geometry in one surface.",
                                TextOptions::new().color(MUTED).font_size(11.0),
                            );
                        },
                    );
                },
            );
        },
    );
}

#[composable]
fn StateLab(note: TextFieldState, counter: MutableState<u32>) {
    let value = note.text();
    let count = counter.get();
    Column(
        ColumnOptions::new().modifier(Modifier::empty().fill_max_size()),
        || {
            Text("State Lab", TextOptions::new().color(INK).font_size(25.0));
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(4.0)));
            Text(
                "Edit a value and watch the retained composition update around it.",
                TextOptions::new().color(MUTED).font_size(12.0),
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(18.0)));
            Row(
                RowOptions::new()
                    .modifier(Modifier::empty().fill_max_width())
                    .vertical_alignment(CrossAxisAlignment::Start),
                || {
                    Column(
                        ColumnOptions::new()
                            .modifier(panel(Modifier::empty().weight(1.0).padding(18.0))),
                        || {
                            Text(
                                "EDITOR BUFFER",
                                TextOptions::new().color(MUTED).font_size(10.0),
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(10.0)));
                            BasicTextField(
                                &note,
                                TextFieldOptions::new().multiline().min_lines(7).modifier(
                                    Modifier::empty()
                                        .fill_max_width()
                                        .background(SOFT)
                                        .border(1.0, LINE)
                                        .border_radius(8.0)
                                        .clip()
                                        .padding(12.0),
                                ),
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(10.0)));
                            Text(
                                format!("{} characters", value.chars().count()),
                                TextOptions::new().color(MUTED).font_size(11.0),
                            );
                        },
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().width(14.0)));
                    Column(
                        ColumnOptions::new()
                            .modifier(soft_panel(Modifier::empty().width(220.0).padding(18.0))),
                        || {
                            Text("COUNTER", TextOptions::new().color(MUTED).font_size(10.0));
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(8.0)));
                            Text(
                                count.to_string(),
                                TextOptions::new().color(ACCENT).font_size(36.0),
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(4.0)));
                            Text(
                                "Each press invalidates only the reader.",
                                TextOptions::new().color(MUTED).font_size(11.0),
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(16.0)));
                            let increment = counter.clone();
                            Button(
                                move || increment.set(increment.get() + 1),
                                ButtonOptions::new().modifier(Modifier::empty().fill_max_width()),
                                || {
                                    Text(
                                        "INCREMENT",
                                        TextOptions::new().color(Color::WHITE).font_size(11.0),
                                    )
                                },
                            );
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(7.0)));
                            let clear = note.clone();
                            Button(
                                move || clear.set_value(TextFieldValue::new("")),
                                ButtonOptions::new().modifier(
                                    Modifier::empty()
                                        .fill_max_width()
                                        .background(SURFACE)
                                        .border(1.0, LINE),
                                ),
                                || {
                                    Text(
                                        "CLEAR BUFFER",
                                        TextOptions::new().color(INK).font_size(11.0),
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

#[composable]
fn ModifierLab() {
    let taps = remember_mutable_state(|| 0_u32);
    let tap_count = taps.get();
    Column(
        ColumnOptions::new().modifier(Modifier::empty().fill_max_size()),
        || {
            Text(
                "Modifier Lab",
                TextOptions::new().color(INK).font_size(25.0),
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(4.0)));
            Text(
                "The same content, with different outer and inner paint layers.",
                TextOptions::new().color(MUTED).font_size(12.0),
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(18.0)));
            Row(
                RowOptions::new()
                    .modifier(Modifier::empty().fill_max_width())
                    .vertical_alignment(CrossAxisAlignment::Start),
                || {
                    LayerExample(
                        "PADDING -> BACKGROUND",
                        "The fill stays inside the padding layer.",
                        Modifier::empty()
                            .padding(18.0)
                            .background(ACCENT_SOFT)
                            .border(1.0, ACCENT),
                        ACCENT,
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().width(14.0)));
                    LayerExample(
                        "BACKGROUND -> PADDING",
                        "The fill owns the outer surface.",
                        Modifier::empty()
                            .background(GREEN_SOFT)
                            .border(1.0, GREEN)
                            .padding(18.0),
                        GREEN,
                    );
                },
            );
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(14.0)));
            Column(
                ColumnOptions::new()
                    .modifier(panel(Modifier::empty().fill_max_width().padding(18.0))),
                || {
                    Row(
                        RowOptions::new()
                            .modifier(Modifier::empty().fill_max_width())
                            .horizontal_arrangement(Arrangement::SpaceBetween)
                            .vertical_alignment(CrossAxisAlignment::Center),
                        || {
                            Column(ColumnOptions::new(), || {
                                Text(
                                    "INTERACTION LAYER",
                                    TextOptions::new().color(MUTED).font_size(10.0),
                                );
                                Spacer(
                                    SpacerOptions::new().modifier(Modifier::empty().height(5.0)),
                                );
                                Text(
                                    "Tap the control to invalidate this panel.",
                                    TextOptions::new().color(INK).font_size(13.0),
                                );
                            });
                            let taps = taps.clone();
                            Button(
                                move || taps.set(taps.get() + 1),
                                ButtonOptions::new()
                                    .modifier(Modifier::empty().min_size(96.0, 38.0)),
                                || {
                                    Text(
                                        "TAP",
                                        TextOptions::new().color(Color::WHITE).font_size(11.0),
                                    )
                                },
                            );
                        },
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().height(12.0)));
                    Text(
                        format!("interaction count  /  {tap_count}"),
                        TextOptions::new().color(ACCENT).font_size(12.0),
                    );
                },
            );
        },
    );
}

#[composable]
fn LayerExample(title: &'static str, detail: &'static str, modifier: Modifier, color: Color) {
    Column(
        ColumnOptions::new().modifier(panel(Modifier::empty().weight(1.0).padding(16.0))),
        || {
            Text(title, TextOptions::new().color(color).font_size(10.0));
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(9.0)));
            Box(BoxOptions::new().modifier(modifier), || {
                Text("LAYER", TextOptions::new().color(INK).font_size(13.0));
            });
            Spacer(SpacerOptions::new().modifier(Modifier::empty().height(10.0)));
            Text(detail, TextOptions::new().color(MUTED).font_size(11.0));
        },
    );
}

#[composable]
fn Console() {
    let screen = remember_mutable_state(|| Screen::Overview);
    let tasks = remember_mutable_state(initial_tasks);
    let next_id = remember_mutable_state(|| 5_u64);
    let filter = remember_mutable_state(|| TaskFilter::All);
    let list_scroll = remember_scroll_state(0.0);
    let note = remember_text_field_state(
        "Compose state is readable, editable and retained across recomposition.",
    );
    let counter = remember_mutable_state(|| 0_u32);
    let tasks_snapshot = tasks.get();
    let selected = screen.get();

    Surface(
        SurfaceOptions::new().modifier(
            Modifier::empty()
                .fill_max_size()
                .background(CANVAS)
                .padding(22.0),
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
                                    .min_size(0.0, 58.0)
                                    .background(INK)
                                    .border_radius(9.0)
                                    .clip()
                                    .padding(16.0),
                            )
                            .horizontal_arrangement(Arrangement::SpaceBetween)
                            .vertical_alignment(CrossAxisAlignment::Center),
                        || {
                            Row(
                                RowOptions::new().vertical_alignment(CrossAxisAlignment::Center),
                                || {
                                    Text(
                                        "KARU CONSOLE",
                                        TextOptions::new().color(Color::WHITE).font_size(15.0),
                                    );
                                    Spacer(
                                        SpacerOptions::new()
                                            .modifier(Modifier::empty().width(10.0)),
                                    );
                                    Text(
                                        "0.1",
                                        TextOptions::new()
                                            .color(Color::rgb(0.55, 0.64, 0.78))
                                            .font_size(10.0),
                                    );
                                },
                            );
                            Text(
                                "WGPU  /  COMMAND STREAM ONLINE",
                                TextOptions::new()
                                    .color(Color::rgb(0.55, 0.64, 0.78))
                                    .font_size(10.0),
                            );
                        },
                    );
                    Spacer(SpacerOptions::new().modifier(Modifier::empty().height(14.0)));
                    Row(
                        RowOptions::new()
                            .modifier(Modifier::empty().fill_max_width().weight(1.0))
                            .vertical_alignment(CrossAxisAlignment::Start),
                        || {
                            Sidebar(screen.clone(), &tasks_snapshot);
                            Spacer(SpacerOptions::new().modifier(Modifier::empty().width(14.0)));
                            key(selected as u8, || match selected {
                                Screen::Overview => Overview(
                                    tasks.clone(),
                                    next_id.clone(),
                                    filter.clone(),
                                    list_scroll.clone(),
                                ),
                                Screen::StateLab => StateLab(note.clone(), counter.clone()),
                                Screen::ModifierLab => ModifierLab(),
                            });
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
        .title("Karu Console")
        .size(1180, 760)
        .background(CANVAS)
        .build()
        .run(Console);
}
