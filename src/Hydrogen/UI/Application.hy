using System;
using System.IO;

namespace Hydrogen.UI {
    // The desktop host starts Main once to locate the view, then once per event.
    // Widget values are supplied as an argument snapshot; no shell is involved.
    public class Application {
        public static void Open(string document) {
            System.Console.WriteLine("HYUI\tOPEN\t" + Escape(document));
        }

        public static bool IsEvent(string[] args) {
            return args.Length >= 3 && args[0] == "--hyui-event";
        }

        public static string Handler(string[] args) {
            if (IsEvent(args)) { return args[1]; }
            return "";
        }

        public static string Sender(string[] args) {
            if (IsEvent(args)) { return args[2]; }
            return "";
        }

        public static string Value(string[] args, string id) {
            int i = 3;
            while (i + 1 < args.Length) {
                if (args[i] == id) { return args[i + 1]; }
                i = i + 2;
            }
            return "";
        }

        // Only the password field's own event receives its one-time private file.
        public static string SecretValue(string[] args, string id) {
            if (Sender(args) != id) { return ""; }
            int i = 3;
            while (i + 1 < args.Length) {
                if (args[i] == "--hyui-secret:" + id) { return System.IO.File.ReadAllText(args[i + 1]); }
                i = i + 2;
            }
            return "";
        }

        public static void Set(string id, string property, string value) {
            System.Console.WriteLine("HYUI\tSET\t" + Escape(id) + "\t" + Escape(property) + "\t" + Escape(value));
        }

        // Ask the native host to close its main window after an event finishes.
        // Useful for launcher-to-workspace transitions; not a process kill.
        public static void Exit() { System.Console.WriteLine("HYUI\tEXIT"); }

        public static void SetText(string id, string text) { Set(id, "text", text); }
        public static void SetPath(string id, string path) { Set(id, "path", path); }
        public static void SetValue(string id, int value) { Set(id, "value", "" + value); }
        public static void SetChecked(string id, bool value) { Set(id, "checked", "" + value); }
        public static void SetEnabled(string id, bool value) { Set(id, "enabled", "" + value); }
        public static void SetVisible(string id, bool value) { Set(id, "visible", "" + value); }
        public static void SetOpen(string id, bool value) { Set(id, "open", "" + value); }
        public static void SetRunning(string id, bool value) { Set(id, "running", "" + value); }

        public static void ClipboardWrite(string id, string value) {
            System.Console.WriteLine("HYUI\tCLIPBOARD\tWRITE\t" + Escape(id) + "\t" + Escape(value));
        }
        public static void ClipboardRead(string id) {
            System.Console.WriteLine("HYUI\tCLIPBOARD\tREAD\t" + Escape(id));
        }

        private static string Escape(string value) {
            string result = "";
            int i = 0;
            while (i < value.Length) {
                string ch = value[i];
                if (ch == "%") { result = result + "%25"; }
                else if (ch == "\t") { result = result + "%09"; }
                else if (ch == "\n") { result = result + "%0A"; }
                else if (ch == "\r") { result = result + "%0D"; }
                else { result = result + ch; }
                i = i + 1;
            }
            return result;
        }
    }
}
