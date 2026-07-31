#[allow(unused_imports)]
use karu::{App, Column, Text, composable, mangled_composable, remember_state};
use karu_backend_quad::Quad;

#[composable]
fn TodoItem(todo: String, index: i32) {
    Text(format!("Item #{index}: {todo}"));
}

#[composable]
fn TodoApp() {
    let todos = remember_state(|| vec!["what's going on?".to_string(), "何意味".to_string()]);

    Column(|| {
        for (idx, todo) in todos.iter().enumerate() {
            TodoItem(todo, idx as i32);
        }
    });
}

fn main() {
    App::builder()
        .with_renderer(Quad.default_system_font())
        .title("Karu Quad Demo")
        .build()
        .run(mangled_composable!(TodoApp));
}
