#[allow(unused_imports)]
use karu::{Column, Composition, Text, composable, remember_state};

#[composable]
fn TodoItem(todo: String) {
    Text(todo);
}

#[composable]
fn TodoApp() {
    let todos = remember_state(|| vec!["write runtime".to_string(), "test renderer".to_string()]);

    Column(|| {
        for todo in todos.iter() {
            TodoItem(todo);
        }
    });
}

fn main() {
    let mut composition = Composition::new(TodoApp);
    let result = composition.compose();
    println!("{:#?}", result.commands);
}
