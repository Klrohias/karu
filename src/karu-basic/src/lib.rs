use karu::{Alignment, BoxOptions, Modifier, Role, composable};

pub struct ButtonOptions {
    modifier: Modifier,
    enabled: bool,
}

impl Default for ButtonOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Default)]
pub struct SurfaceOptions {
    modifier: Modifier,
}

impl SurfaceOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }
}

impl ButtonOptions {
    pub fn new() -> Self {
        Self {
            modifier: Modifier::empty(),
            enabled: true,
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

pub mod style {
    use karu::{Brush, Color, InteractionState, Modifier, ModifierData, ModifierElement};
    use std::any::Any;

    pub const PRIMARY: Color = Color::rgb(0.12, 0.36, 0.86);
    pub const PRIMARY_HOVER: Color = Color::rgb(0.18, 0.43, 0.94);
    pub const PRIMARY_PRESSED: Color = Color::rgb(0.08, 0.27, 0.68);

    #[derive(Debug)]
    pub struct ButtonBackground;

    impl ModifierElement for ButtonBackground {
        fn apply(&self, data: &mut ModifierData) {
            self.resolve(data, InteractionState::default());
        }

        fn resolve(&self, data: &mut ModifierData, interaction: InteractionState) {
            let color = if interaction.pressed {
                PRIMARY_PRESSED
            } else if interaction.hovered {
                PRIMARY_HOVER
            } else {
                PRIMARY
            };
            data.background = Some(color);
            data.background_brush = Some(Brush::Solid(color));
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    pub fn button(modifier: Modifier) -> Modifier {
        Modifier::empty()
            .min_size(64.0, 20.0)
            .padding(8.0)
            .then(ButtonBackground)
            .border(1.0, Color::rgb(0.06, 0.20, 0.55))
            .border_radius(4.0)
            .clip()
            .then_modifier(modifier)
    }
}

#[allow(non_snake_case)]
#[composable]
pub fn Button(on_click: impl FnMut() + 'static, options: ButtonOptions, content: impl FnMut()) {
    let modifier = style::button(options.modifier).role(Role::Button);
    let modifier = if options.enabled {
        modifier.clickable(on_click)
    } else {
        modifier.disabled()
    };
    Box(BoxOptions::new().modifier(modifier), content);
}

#[allow(non_snake_case)]
#[composable]
pub fn Surface(options: SurfaceOptions, content: impl FnMut()) {
    Box(BoxOptions::new().modifier(options.modifier), content);
}

pub fn centered(modifier: Modifier) -> Modifier {
    modifier.align(Alignment::Center)
}
