#[allow(unused_imports)]
use karu::{App, Column, Text, composable, remember_state};
use karu_backend_quad::Quad;

#[composable]
fn TodoItem(todo: String) {
    Text(todo);
}

#[composable]
fn TodoApp() {
    let todos = remember_state(|| vec!["what's going on?".to_string(), "hyw".to_string()]);

    Column(|| {
        for todo in todos.iter() {
            TodoItem(todo);
        }
    });
}

fn main() {
    App::builder()
        .with_renderer(Quad.system_font(""))
        .title("Karu Quad Demo")
        .build()
        .run(__karu_TodoApp);
}
