# stand-in.py <interface> <glance>: a stand-in for Pitboard on the bus tst_widget.qml runs on,
# which the bus starts by the service file run-tests.sh writes, and only once something asks for
# Pitboard's name. It owns that name and serves what the app serves a widget: the widget feed,
# as the app's own interface declares it, holding the glance given, and the app's
# org.freedesktop.Application, keeping each action it is asked for. Its own interface, at
# /StandIn, feeds another glance, as the app does once it reads again, says what it was asked
# for, as JSON, and ends it.
import json
import sys
from pathlib import Path

from gi.repository import Gio, GLib

NAME = "com.usepitboard.Pitboard"
APP = "/com/usepitboard/Pitboard"
FEED = "com.usepitboard.Pitboard.WidgetFeed"
STAND_IN = "com.usepitboard.Pitboard.StandIn"
# org.freedesktop.Application as GApplication serves it, and the stand-in's own.
SERVED = f"""<node>
  <interface name="org.freedesktop.Application">
    <method name="Activate">
      <arg type="a{{sv}}" name="platform_data" direction="in"/>
    </method>
    <method name="Open">
      <arg type="as" name="uris" direction="in"/>
      <arg type="s" name="hint" direction="in"/>
      <arg type="a{{sv}}" name="platform_data" direction="in"/>
    </method>
    <method name="ActivateAction">
      <arg type="s" name="action_name" direction="in"/>
      <arg type="av" name="parameter" direction="in"/>
      <arg type="a{{sv}}" name="platform_data" direction="in"/>
    </method>
  </interface>
  <interface name="{STAND_IN}">
    <method name="Feed">
      <arg type="s" name="glance" direction="in"/>
    </method>
    <method name="Asked">
      <arg type="s" name="asked" direction="out"/>
    </method>
    <method name="Quit"/>
  </interface>
</node>"""


def main():
    interface, glance = (Path(argument) for argument in sys.argv[1:])
    feed = Gio.DBusNodeInfo.new_for_xml(interface.read_text(encoding="utf-8"))
    served = Gio.DBusNodeInfo.new_for_xml(SERVED)
    fed = {"Feed": glance.read_text(encoding="utf-8"), "Revision": 1}
    asked = []
    loop = GLib.MainLoop()

    def feed_property(connection, sender, path, interface, name):
        return GLib.Variant("s" if name == "Feed" else "t", fed[name])

    def application(connection, sender, path, interface, method, parameters, invocation):
        if method == "ActivateAction":
            name, parameter, _ = parameters.unpack()
            asked.append([name, parameter])
        invocation.return_value(None)

    def stand_in(connection, sender, path, interface, method, parameters, invocation):
        if method == "Feed":
            (fed["Feed"],) = parameters.unpack()
            fed["Revision"] += 1
            changed = {
                "Feed": GLib.Variant("s", fed["Feed"]),
                "Revision": GLib.Variant("t", fed["Revision"]),
            }
            connection.emit_signal(
                None, f"{APP}/WidgetFeed", "org.freedesktop.DBus.Properties", "PropertiesChanged",
                GLib.Variant("(sa{sv}as)", (FEED, changed, [])),
            )
            invocation.return_value(None)
        elif method == "Asked":
            invocation.return_value(GLib.Variant("(s)", (json.dumps(asked),)))
        else:
            invocation.return_value(None)
            GLib.idle_add(loop.quit)

    # Served before the name is owned, so whoever sees the owner finds them.
    def on_the_bus(connection, name):
        connection.register_object_with_closures2(
            f"{APP}/WidgetFeed", feed.lookup_interface(FEED), None, feed_property, None
        )
        connection.register_object_with_closures2(
            APP, served.lookup_interface("org.freedesktop.Application"), application, None, None
        )
        connection.register_object_with_closures2(
            "/StandIn", served.lookup_interface(STAND_IN), stand_in, None, None
        )

    Gio.bus_own_name(
        Gio.BusType.SESSION, NAME, Gio.BusNameOwnerFlags.NONE, on_the_bus, None,
        lambda connection, name: loop.quit(),
    )
    loop.run()


if __name__ == "__main__":
    main()
