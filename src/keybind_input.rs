use iced::{
    Element, Event, Rectangle, Renderer, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        layout::Node,
        mouse::{Cursor, Interaction},
        widget::{
            Operation, Tree,
            tree::{State as TreeState, Tag},
        },
    },
    keyboard::{self, Key, Modifiers, key::Named},
    mouse,
    widget::{TextInput, text_input},
};

use crate::{Message, format_key, format_modifiers};

pub struct KeybindInput<'a> {
    child: TextInput<'a, Message>,
    on_input: Box<dyn Fn(String) -> Message + 'a>,
    last_value: &'a str,
    default: Option<&'a str>,
    optional: bool,
}

pub struct State {
    prev: Option<Box<str>>,
}

impl<'a> KeybindInput<'a> {
    pub fn new(
        value: &'a str,
        default: Option<&'a str>,
        on_input: impl Fn(String) -> Message + Clone + 'a,
        optional: bool,
    ) -> Self {
        let child = iced::widget::text_input("Input Key...", value).on_input(on_input.clone());
        Self {
            child,
            default,
            on_input: Box::new(on_input),
            last_value: value,
            optional,
        }
    }
}

impl Widget<Message, Theme, Renderer> for KeybindInput<'_> {
    fn tag(&self) -> Tag {
        Tag::of::<State>()
    }

    fn state(&self) -> TreeState {
        TreeState::new(State { prev: None })
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[(&self.child as &dyn Widget<_, _, _>)]);
    }

    fn size(&self) -> iced::Size<iced::Length> {
        Widget::size(&self.child)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child as &dyn Widget<_, _, _>)]
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> Node {
        let child = Widget::layout(
            &mut self.child,
            tree.children.first_mut().unwrap(),
            renderer,
            limits,
        );
        Node::with_children(child.size(), vec![child])
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        Widget::draw(
            &self.child,
            tree.children.first().unwrap(),
            renderer,
            theme,
            style,
            layout.children().next().unwrap(),
            cursor,
            viewport,
        );
    }

    fn size_hint(&self) -> iced::Size<iced::Length> {
        self.child.size_hint()
    }

    fn operate(
        &mut self,
        state: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child.operate(
            state.children.first_mut().unwrap(),
            layout,
            renderer,
            operation,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let text_input_tree = tree.children.first_mut().unwrap();
        let text_input_state = input_state(text_input_tree);

        if state.prev.is_some() {
            shell.capture_event();
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(btn)) => {
                    let v = state.prev.take().unwrap();
                    shell.publish((self.on_input)(v.into_string()));

                    if *btn != mouse::Button::Right || !cursor.is_over(layout.bounds()) {
                        text_input_state.unfocus();
                    }
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    repeat: false,
                    ..
                }) => match key {
                    Key::Named(
                        k @ (Named::Super
                        | Named::Meta
                        | Named::Hyper
                        | Named::Alt
                        | Named::Control
                        | Named::Shift),
                    ) => {
                        let modifier = match k {
                            Named::Super | Named::Meta | Named::Hyper => Modifiers::LOGO,
                            Named::Alt => Modifiers::ALT,
                            Named::Control => Modifiers::CTRL,
                            Named::Shift => Modifiers::SHIFT,
                            _ => unreachable!(),
                        };
                        let mut key = String::new();
                        format_modifiers(*modifiers | modifier, &mut key);
                        shell.publish((self.on_input)(key));
                    }
                    Key::Named(Named::Backspace) => {
                        let v = if let Some(v) = &self.default
                            && modifiers.command()
                        {
                            v.to_string()
                        } else if self.optional {
                            String::new()
                        } else if let Some(v) = &self.default {
                            v.to_string()
                        } else {
                            return;
                        };

                        shell.publish((self.on_input)(v));

                        state.prev = None;
                        text_input_state.unfocus();
                    }
                    Key::Named(Named::Escape) => {
                        let prev = state.prev.take().unwrap();
                        shell.publish((self.on_input)(prev.into_string()));

                        state.prev = None;
                        text_input_state.unfocus();
                    }
                    Key::Unidentified => {}

                    #[allow(clippy::match_same_arms)]
                    _ => {
                        let key = format_key(key, *modifiers);
                        shell.publish((self.on_input)(key));

                        state.prev = None;
                        text_input_state.unfocus();
                    }
                },
                Event::Window(_) => self.child.update(
                    text_input_tree,
                    event,
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                ),

                _ => (),
            }
            return;
        }

        if let Event::Mouse(mouse::Event::ButtonPressed(button)) = event
            && cursor.is_over(layout.bounds())
            && !text_input_state.is_focused()
        {
            match button {
                mouse::Button::Left => {
                    text_input_state.focus();
                    state.prev = Some(self.last_value.into());
                    shell.publish((self.on_input)(String::new()));
                    return;
                }
                mouse::Button::Right => {
                    self.child.update(
                        text_input_tree,
                        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        layout,
                        cursor,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    );
                    return;
                }
                _ => (),
            }
        }
        if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) = event {
            self.child.update(
                text_input_tree,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        self.child.update(
            text_input_tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        _: &Tree,
        _: Layout<'_>,
        _: Cursor,
        _: &Rectangle,
        _: &Renderer,
    ) -> Interaction {
        Interaction::Idle
    }
}

impl<'a> From<KeybindInput<'a>> for Element<'a, Message> {
    fn from(val: KeybindInput<'a>) -> Self {
        Element::new(val)
    }
}

fn input_state(
    tree: &mut Tree,
) -> &mut text_input::State<<Renderer as iced::advanced::text::Renderer>::Paragraph> {
    tree.state
        .downcast_mut::<text_input::State<<Renderer as iced::advanced::text::Renderer>::Paragraph>>(
        )
}
