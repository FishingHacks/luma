use iced::{
    Border, Element, Event, Rectangle, Renderer, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        mouse::{Cursor, Interaction},
        widget::{Operation, Tree},
    },
    keyboard::{self, Key, Modifiers, key::Named},
    mouse,
    widget::{
        TextInput,
        text_input::{self, Id},
    },
};

use crate::{ALLOWED_ACTION_MODIFIERS, Message};

pub struct SearchInput<'a>(TextInput<'a, Message>);

impl SearchInput<'_> {
    pub fn new(query: &str, id: Id) -> Self {
        let inner = iced::widget::text_input("Search", query)
            .id(id)
            .on_input(Message::UpdateSearch)
            .style(|theme, status| {
                let mut style = text_input::default(theme, status);
                style.border = Border::default().width(0.0);
                style
            });
        Self(inner)
    }
}

impl Widget<Message, Theme, Renderer> for SearchInput<'_> {
    fn size(&self) -> iced::Size<iced::Length> {
        Widget::size(&self.0)
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        Widget::layout(&self.0, tree, renderer, limits)
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
            &self.0, tree, renderer, theme, style, layout, cursor, viewport,
        );
    }

    fn size_hint(&self) -> iced::Size<iced::Length> {
        self.0.size_hint()
    }

    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        self.0.tag()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        self.0.state()
    }

    fn children(&self) -> Vec<Tree> {
        self.0.children()
    }

    fn operate(
        &self,
        state: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.0.operate(state, layout, renderer, operation);
    }

    fn update(
        &mut self,
        state: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let handled = 'blk: {
            match event {
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key: Key::Named(Named::Alt),
                    ..
                }) => {
                    shell.publish(Message::HideActions);
                }
                Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    let is_ctrl = *modifiers == Modifiers::CTRL;
                    let is_ctrl_shift = *modifiers == Modifiers::CTRL.union(Modifiers::SHIFT);
                    match key {
                        Key::Named(Named::ArrowLeft | Named::ArrowRight | Named::Backspace)
                            if is_ctrl || is_ctrl_shift =>
                        {
                            break 'blk false;
                        }
                        Key::Character(c)
                            if is_ctrl && (c == "a" || c == "c" || c == "x" || c == "v") =>
                        {
                            break 'blk false;
                        }
                        Key::Named(Named::Enter)
                            // only no modifiers or alt+enter count as submit (alt because of the
                            // actions list that shows up when holding down alt.)
                            if (*modifiers | Modifiers::ALT) == Modifiers::ALT =>
                        {
                            shell.publish(Message::Submit);
                        }
                        Key::Named(Named::PageUp) => shell.publish(Message::Go10Up),
                        Key::Named(Named::PageDown) => shell.publish(Message::Go10Down),
                        Key::Named(Named::ArrowUp) => shell.publish(Message::GoUp),
                        Key::Named(Named::ArrowDown) => shell.publish(Message::GoDown),
                        Key::Named(Named::Escape) => shell.publish(Message::HideMainWindow),
                        Key::Named(Named::Alt) => shell.publish(Message::ShowActions),
                        Key::Named(Named::Tab) => {
                            shell.publish(Message::KeyPressed(Key::Named(Named::Tab), *modifiers));
                        }
                        _ if ALLOWED_ACTION_MODIFIERS.intersects(*modifiers) => {
                            shell.publish(Message::KeyPressed(key.clone(), *modifiers));
                        }
                        _ => break 'blk false,
                    }
                }
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                    if cursor.position_over(layout.bounds()).is_some() =>
                {
                    shell.publish(Message::InputPress);
                }

                _ => break 'blk false,
            }
            true
        };
        if handled {
            shell.capture_event();
            return;
        }
        self.0.update(
            state, event, layout, cursor, renderer, clipboard, shell, viewport,
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

impl<'a> From<SearchInput<'a>> for Element<'a, Message> {
    fn from(val: SearchInput<'a>) -> Self {
        Element::new(val)
    }
}
