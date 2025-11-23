use accesskit::Role;
use bevy_a11y::AccessibilityNode;
use bevy_app::{App, Plugin};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::event::EntityEvent;
use bevy_ecs::lifecycle::Insert;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Has, With};
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::DeferredWorld;
use bevy_input::keyboard::{KeyCode, KeyboardInput};
use bevy_input::ButtonInput;
use bevy_input::ButtonState;
use bevy_input_focus::{FocusedInput, InputFocus, InputFocusVisible};
use bevy_math::ops;
use bevy_picking::events::{Click, Drag, DragEnd, DragStart, Pointer, Scroll};
use bevy_ui::{InteractionDisabled, UiGlobalTransform, UiScale};

use crate::ValueChange;

/// Headless widget that implements the interaction logic used by both numeric and
/// discrete spinboxes. The widget has no built-in visuals: users are expected to provide
/// their own layout and styling.
///
/// The [`Spinbox`] emits [`ValueChange<f32>`] events whenever the stored [`SpinboxValue`] needs to
/// change. This value is just a scalar number. Applications can interpret it however they like:
/// * as a floating point quantity,
/// * as an integer index into an enum, or
/// * as a parameter that drives a custom data binding.
///
/// The widget supports keyboard input, scroll wheel adjustment, and pointer dragging anywhere
/// inside the widget. Modifiers allow fine and coarse control over both dragging and keyboard input.
#[derive(Component, Debug, Default)]
#[require(
    AccessibilityNode(accesskit::Node::new(Role::SpinButton)),
    SpinboxValue,
    SpinboxRange,
    SpinboxStep,
    SpinboxSensitivity,
    SpinboxDragState
)]
pub struct Spinbox {
    /// When `true`, values that leave the [`SpinboxRange`] wrap around instead of clamping.
    pub wrap: bool,
}

/// Stores the current logical value of a [`Spinbox`].
#[derive(Component, Debug, Default, PartialEq, Clone, Copy)]
#[component(immutable)]
pub struct SpinboxValue(pub f32);

/// Defines the minimum and maximum allowed values for a [`Spinbox`].
#[derive(Component, Debug, PartialEq, Clone, Copy)]
#[component(immutable)]
pub struct SpinboxRange {
    start: f32,
    end: f32,
}

impl SpinboxRange {
    /// Creates a new range.
    pub fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }

    /// Beginning of the range.
    pub fn start(&self) -> f32 {
        self.start
    }

    /// End of the range.
    pub fn end(&self) -> f32 {
        self.end
    }

    /// Span of the range.
    pub fn span(&self) -> f32 {
        self.end - self.start
    }

    fn clamp(&self, value: f32) -> f32 {
        value.clamp(self.start.min(self.end), self.start.max(self.end))
    }

    fn wrap(&self, mut value: f32) -> f32 {
        let span = self.end - self.start;
        if span == 0.0 {
            return self.start;
        }
        let (min, max) = if span.is_sign_positive() {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        };
        if value < min || value > max {
            value = (value - min).rem_euclid(span.abs()) + min;
        }
        value
    }
}

impl Default for SpinboxRange {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 1.0,
        }
    }
}

/// Defines the amount by which to update the value when responding to keyboard input or scrolling.
#[derive(Component, Debug, PartialEq, Clone, Copy)]
#[component(immutable)]
pub struct SpinboxStep {
    /// Step used when no modifier is held.
    pub normal: f32,
    /// Step used while holding the `Shift` key.
    pub fine: f32,
    /// Step used while holding the `Ctrl` or `Alt` key.
    pub coarse: f32,
}

impl Default for SpinboxStep {
    fn default() -> Self {
        Self {
            normal: 1.0,
            fine: 0.1,
            coarse: 10.0,
        }
    }
}

impl SpinboxStep {
    fn amount(&self, modifier: SpinboxModifier) -> f32 {
        match modifier {
            SpinboxModifier::Fine => self.fine,
            SpinboxModifier::Normal => self.normal,
            SpinboxModifier::Coarse => self.coarse,
        }
    }
}

/// Controls how pointer drags translate into value changes.
#[derive(Component, Debug, PartialEq, Clone, Copy)]
#[component(immutable)]
pub struct SpinboxSensitivity {
    /// Base value change per pixel dragged horizontally.
    pub normal_per_pixel: f32,
    /// Value change per pixel while holding `Shift`.
    pub fine_per_pixel: f32,
    /// Value change per pixel while holding `Ctrl` or `Alt`.
    pub coarse_per_pixel: f32,
}

impl Default for SpinboxSensitivity {
    fn default() -> Self {
        Self {
            normal_per_pixel: 0.01,
            fine_per_pixel: 0.001,
            coarse_per_pixel: 0.1,
        }
    }
}

impl SpinboxSensitivity {
    fn per_pixel(&self, modifier: SpinboxModifier) -> f32 {
        match modifier {
            SpinboxModifier::Fine => self.fine_per_pixel,
            SpinboxModifier::Normal => self.normal_per_pixel,
            SpinboxModifier::Coarse => self.coarse_per_pixel,
        }
    }
}

/// Optional component that rounds [`SpinboxValue`] during interactions.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct SpinboxPrecision(pub i32);

impl SpinboxPrecision {
    fn round(&self, value: f32) -> f32 {
        let factor = ops::powf(10.0_f32, self.0 as f32);
        (value * factor).round() / factor
    }
}

/// Internal component used to track drag state.
#[derive(Component, Debug, Default)]
pub struct SpinboxDragState {
    /// Whether the spinbox is currently being dragged.
    pub dragging: bool,
}

/// Event that can be triggered to modify a [`Spinbox`] value.
#[derive(EntityEvent, Clone)]
pub struct SetSpinboxValue {
    /// The [`Spinbox`] entity to update.
    pub entity: Entity,
    /// The change to apply.
    pub change: SpinboxValueChange,
}

/// Different ways to update a spinbox value via [`SetSpinboxValue`].
#[derive(Clone)]
pub enum SpinboxValueChange {
    /// Set the value to an absolute number.
    Absolute(f32),
    /// Add a delta to the value.
    Relative(f32),
    /// Add a delta scaled by the configured [`SpinboxStep::normal`].
    RelativeStep(f32),
}

/// Registers observers required for the [`Spinbox`] widget.
pub struct SpinboxPlugin;

impl Plugin for SpinboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spinbox_on_pointer_click)
            .add_observer(spinbox_on_drag_start)
            .add_observer(spinbox_on_drag)
            .add_observer(spinbox_on_drag_end)
            .add_observer(spinbox_on_scroll)
            .add_observer(spinbox_on_key_input)
            .add_observer(spinbox_on_insert_value)
            .add_observer(spinbox_on_insert_range)
            .add_observer(spinbox_on_insert_step)
            .add_observer(spinbox_on_set_value);
    }
}

fn spinbox_on_pointer_click(
    mut click: On<Pointer<Click>>,
    q_spinbox: Query<Has<InteractionDisabled>, With<Spinbox>>,
    focus: Option<ResMut<InputFocus>>,
    focus_visible: Option<ResMut<InputFocusVisible>>,
) {
    if let Ok(disabled) = q_spinbox.get(click.entity) {
        click.propagate(false);
        if disabled {
            return;
        }
        if let Some(mut focus) = focus {
            focus.0 = Some(click.entity);
        }
        if let Some(mut focus_visible) = focus_visible {
            focus_visible.0 = false;
        }
    }
}

fn spinbox_on_drag_start(
    mut drag_start: On<Pointer<DragStart>>,
    mut q_spinbox: Query<(&mut SpinboxDragState, Has<InteractionDisabled>), With<Spinbox>>,
) {
    if let Ok((mut drag_state, disabled)) = q_spinbox.get_mut(drag_start.entity) {
        drag_start.propagate(false);
        if disabled {
            return;
        }
        drag_state.dragging = true;
    }
}

fn spinbox_on_drag(
    mut drag: On<Pointer<Drag>>,
    mut q_spinbox: Query<
        (
            &Spinbox,
            &SpinboxRange,
            &SpinboxSensitivity,
            Option<&SpinboxPrecision>,
            &SpinboxValue,
            &mut SpinboxDragState,
            Has<InteractionDisabled>,
            &UiGlobalTransform,
        ),
        With<Spinbox>,
    >,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut commands: Commands,
    ui_scale: Res<UiScale>,
) {
    if let Ok((spinbox, range, sensitivity, precision, value, drag_state, disabled, transform)) =
        q_spinbox.get_mut(drag.entity)
    {
        drag.propagate(false);
        if disabled || !drag_state.dragging {
            return;
        }

        let modifier = detect_modifier(keys.as_ref().map(|res| res.as_ref()));
        let mut delta = drag.delta / ui_scale.0;
        delta.y *= -1.0;
        let delta = transform.transform_vector2(delta);
        let per_pixel = sensitivity.per_pixel(modifier);
        let mut new_value = value.0 + delta.x * per_pixel;
        if let Some(precision) = precision {
            new_value = precision.round(new_value);
        }
        new_value = apply_range(new_value, range, spinbox.wrap);
        if new_value != value.0 {
            commands.trigger(ValueChange {
                source: drag.entity,
                value: new_value,
            });
        }
    }
}

fn spinbox_on_drag_end(
    mut drag_end: On<Pointer<DragEnd>>,
    mut q_spinbox: Query<&mut SpinboxDragState, With<Spinbox>>,
) {
    if let Ok(mut drag_state) = q_spinbox.get_mut(drag_end.entity) {
        drag_end.propagate(false);
        drag_state.dragging = false;
    }
}

fn spinbox_on_scroll(
    mut scroll: On<Pointer<Scroll>>,
    q_spinbox: Query<
        (
            &Spinbox,
            &SpinboxRange,
            &SpinboxStep,
            Option<&SpinboxPrecision>,
            &SpinboxValue,
            Has<InteractionDisabled>,
        ),
        With<Spinbox>,
    >,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut commands: Commands,
) {
    if let Ok((spinbox, range, step, precision, value, disabled)) = q_spinbox.get(scroll.entity) {
        scroll.propagate(false);
        if disabled {
            return;
        }
        let modifier = detect_modifier(keys.as_ref().map(|res| res.as_ref()));
        let delta = scroll.y * step.amount(modifier);
        apply_and_trigger(
            scroll.entity,
            spinbox,
            range,
            precision,
            value,
            delta,
            &mut commands,
        );
    }
}

fn spinbox_on_key_input(
    mut focused_input: On<FocusedInput<KeyboardInput>>,
    q_spinbox: Query<
        (
            &Spinbox,
            &SpinboxRange,
            &SpinboxStep,
            Option<&SpinboxPrecision>,
            &SpinboxValue,
            Has<InteractionDisabled>,
        ),
        With<Spinbox>,
    >,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut commands: Commands,
) {
    if let Ok((spinbox, range, step, precision, value, disabled)) =
        q_spinbox.get(focused_input.focused_entity)
    {
        let input_event = &focused_input.input;
        if disabled || input_event.state != ButtonState::Pressed || input_event.repeat {
            return;
        }
        let modifier = detect_modifier(keys.as_ref().map(|res| res.as_ref()));
        let step_amount = step.amount(modifier);
        let delta = match input_event.key_code {
            KeyCode::ArrowUp | KeyCode::ArrowRight => step_amount,
            KeyCode::ArrowDown | KeyCode::ArrowLeft => -step_amount,
            KeyCode::PageUp => step_amount * 10.0,
            KeyCode::PageDown => -step_amount * 10.0,
            KeyCode::Home => range.start() - value.0,
            KeyCode::End => range.end() - value.0,
            _ => return,
        };
        focused_input.propagate(false);
        apply_and_trigger(
            focused_input.focused_entity,
            spinbox,
            range,
            precision,
            value,
            delta,
            &mut commands,
        );
    }
}

fn spinbox_on_insert_value(insert: On<Insert, SpinboxValue>, mut world: DeferredWorld) {
    let mut entity = world.entity_mut(insert.entity);
    let value = entity.get::<SpinboxValue>().unwrap().0;
    if let Some(mut accessibility) = entity.get_mut::<AccessibilityNode>() {
        accessibility.set_numeric_value(value.into());
    }
}

fn spinbox_on_insert_range(insert: On<Insert, SpinboxRange>, mut world: DeferredWorld) {
    let mut entity = world.entity_mut(insert.entity);
    let range = *entity.get::<SpinboxRange>().unwrap();
    if let Some(mut accessibility) = entity.get_mut::<AccessibilityNode>() {
        accessibility.set_min_numeric_value(range.start().into());
        accessibility.set_max_numeric_value(range.end().into());
    }
}

fn spinbox_on_insert_step(insert: On<Insert, SpinboxStep>, mut world: DeferredWorld) {
    let mut entity = world.entity_mut(insert.entity);
    let step = entity.get::<SpinboxStep>().unwrap().normal;
    if let Some(mut accessibility) = entity.get_mut::<AccessibilityNode>() {
        accessibility.set_numeric_value_step(step.into());
    }
}

fn spinbox_on_set_value(
    set_value: On<SetSpinboxValue>,
    q_spinbox: Query<
        (
            &Spinbox,
            &SpinboxRange,
            &SpinboxStep,
            Option<&SpinboxPrecision>,
            &SpinboxValue,
        ),
        With<Spinbox>,
    >,
    mut commands: Commands,
) {
    if let Ok((spinbox, range, step, precision, value)) = q_spinbox.get(set_value.entity) {
        let delta = match set_value.change {
            SpinboxValueChange::Absolute(new_value) => new_value - value.0,
            SpinboxValueChange::Relative(delta) => delta,
            SpinboxValueChange::RelativeStep(multiplier) => step.normal * multiplier,
        };
        apply_and_trigger(
            set_value.entity,
            spinbox,
            range,
            precision,
            value,
            delta,
            &mut commands,
        );
    }
}

fn apply_and_trigger(
    entity: Entity,
    spinbox: &Spinbox,
    range: &SpinboxRange,
    precision: Option<&SpinboxPrecision>,
    value: &SpinboxValue,
    delta: f32,
    commands: &mut Commands,
) {
    let mut new_value = value.0 + delta;
    if let Some(precision) = precision {
        new_value = precision.round(new_value);
    }
    new_value = apply_range(new_value, range, spinbox.wrap);
    if new_value != value.0 {
        commands.trigger(ValueChange {
            source: entity,
            value: new_value,
        });
    }
}

fn apply_range(value: f32, range: &SpinboxRange, wrap: bool) -> f32 {
    if wrap {
        range.wrap(value)
    } else {
        range.clamp(value)
    }
}

fn detect_modifier(keys: Option<&ButtonInput<KeyCode>>) -> SpinboxModifier {
    let Some(keys) = keys else {
        return SpinboxModifier::Normal;
    };
    if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        SpinboxModifier::Fine
    } else if keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        || keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])
    {
        SpinboxModifier::Coarse
    } else {
        SpinboxModifier::Normal
    }
}

#[derive(Clone, Copy)]
enum SpinboxModifier {
    Fine,
    Normal,
    Coarse,
}

/// Convenience observer for internal-state spinboxes.
pub fn spinbox_self_update(value_change: On<ValueChange<f32>>, mut commands: Commands) {
    commands
        .entity(value_change.source)
        .insert(SpinboxValue(value_change.value));
}
