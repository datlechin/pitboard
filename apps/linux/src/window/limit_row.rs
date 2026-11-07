//! One limit of an account, as a row of its group: its name, when it resets, a level bar and
//! the share used. The bar's one offset is named for the step the model says the limit is at,
//! so the style sheet tints it by that step and the glue holds no threshold of its own: GTK
//! gives a filled block the name of the offset its value is under as a style class.
//! When offered, pace adds the model's words and an expected-use mark on the same used-share
//! axis. An even pace has words alone; the row's spoken sentence includes them either way.

use std::cell::Cell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pitboard_model::{LimitRow as Limit, PaceStanding, UsageLevel};

/// The offset, and so the style class, of a bar at `level`.
pub fn level_name(level: UsageLevel) -> &'static str {
    match level {
        UsageLevel::Plenty => "plenty",
        UsageLevel::Low => "low",
        UsageLevel::Out => "out",
    }
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/usepitboard/Pitboard/ui/limit-row.ui")]
    pub struct LimitRow {
        #[template_child]
        pub(super) bar: TemplateChild<gtk::LevelBar>,
        #[template_child]
        pub(super) figure: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) pace: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) pace_marker: TemplateChild<gtk::DrawingArea>,
        pub(super) expected: Cell<Option<f64>>,
        pub(super) level: Cell<Option<&'static str>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LimitRow {
        const NAME: &'static str = "PitboardLimitRow";
        type Type = super::LimitRow;
        type ParentType = adw::ActionRow;

        fn class_init(class: &mut Self::Class) {
            class.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LimitRow {
        fn constructed(&self) {
            self.parent_constructed();
            let weak = self.obj().downgrade();
            self.pace_marker
                .set_draw_func(move |marker, context, width, height| {
                    let Some(row) = weak.upgrade() else {
                        return;
                    };
                    let Some(expected) = row.imp().expected.get() else {
                        return;
                    };
                    let share = expected.clamp(0.0, 100.0) / 100.0;
                    let share = if marker.direction() == gtk::TextDirection::Rtl {
                        1.0 - share
                    } else {
                        share
                    };
                    let x = (share * f64::from(width)).clamp(0.5, f64::from(width).max(1.0) - 0.5);
                    let color = marker.color();
                    context.set_source_rgba(
                        f64::from(color.red()),
                        f64::from(color.green()),
                        f64::from(color.blue()),
                        f64::from(color.alpha()),
                    );
                    context.set_line_width(1.0);
                    context.move_to(x, 0.0);
                    context.line_to(x, f64::from(height));
                    let _ = context.stroke();
                });
            // GTK's own steps, which follow the bar's value rather than the model's level.
            for offset in [
                gtk::LEVEL_BAR_OFFSET_LOW,
                gtk::LEVEL_BAR_OFFSET_HIGH,
                gtk::LEVEL_BAR_OFFSET_FULL,
            ] {
                self.bar.remove_offset_value(Some(offset));
            }
        }
    }

    impl WidgetImpl for LimitRow {}
    impl ListBoxRowImpl for LimitRow {}
    impl PreferencesRowImpl for LimitRow {}
    impl ActionRowImpl for LimitRow {}
}

glib::wrapper! {
    pub struct LimitRow(ObjectSubclass<imp::LimitRow>)
        @extends adw::ActionRow, adw::PreferencesRow, gtk::ListBoxRow, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LimitRow {
    fn default() -> LimitRow {
        glib::Object::new()
    }
}

impl LimitRow {
    pub fn show(&self, limit: &Limit) {
        let imp = self.imp();
        self.set_title(&limit.name);
        self.set_subtitle(&limit.resets);
        // A limit past its whole is drawn whole: its figure says how far past.
        imp.bar.set_value(limit.percent.clamp(0.0, 100.0));
        let level = level_name(limit.level);
        if let Some(before) = imp.level.replace(Some(level))
            && before != level
        {
            imp.bar.remove_offset_value(Some(before));
        }
        imp.bar.add_offset_value(level, 100.0);
        imp.figure.set_label(&limit.figure);
        imp.pace
            .set_label(limit.pace.as_ref().map_or("", |pace| pace.said.as_str()));
        imp.pace.set_visible(limit.pace.is_some());
        self.set_tooltip_text(limit.pace.as_ref().map(|pace| pace.help.as_str()));
        let expected = limit
            .pace
            .as_ref()
            .and_then(|pace| (pace.standing != PaceStanding::Even).then_some(pace.expected));
        imp.expected.set(expected);
        imp.pace_marker.set_visible(expected.is_some());
        imp.pace_marker.queue_draw();
        // The row is read as one sentence, as the other apps read a limit.
        self.update_property(&[gtk::accessible::Property::Label(&limit.spoken)]);
    }
}
