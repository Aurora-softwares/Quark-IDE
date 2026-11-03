using System;
using System.IO;
using Hydrogen.UI;

namespace Quark {
    public class StartProgram {
        public static int Main(string[] args) {
            if (!Application.IsEvent(args)) {
                Application.Open("Quark.Start.hyui");
                return 0;
            }
            string handler = Application.Handler(args);
            if (handler == "StartReady" || handler == "SearchRecent") {
                Application.SetText("recentList", Session.RecentItems(Application.Value(args, "recentSearch")));
            }
            if (handler == "About") { Application.SetOpen("aboutDialog", true); }
            if (handler == "PickerCancelled") { Status("No file was selected."); }
            if (handler == "OpenProjectHelp") { Status("Use Open a project or solution under Get started."); }
            if (handler == "ProjectPicked") {
                Launch("P", Application.Value(args, Application.Sender(args)));
            }
            if (handler == "SourcePicked") {
                Launch("S", Application.Value(args, Application.Sender(args)));
            }
            if (handler == "UIPicked") {
                Launch("U", Application.Value(args, Application.Sender(args)));
            }
            if (handler == "FolderPicked") {
                Launch("F", Application.Value(args, Application.Sender(args)));
            }
            if (handler == "Continue") { Launch("E", ""); }
            if (handler == "OpenRecent") {
                int number = System.Convert.ToInt32(Application.Value(args, "recentNumber")) - 1;
                string path = Session.RecentAt(number);
                if (path == "") { Status("Choose a recent entry numbered 1 to 10."); }
                else if (System.IO.File.Exists(path)) {
                    string kind = "S";
                    if (EndsWith(path, ".hyproj")) { kind = "P"; }
                    else if (EndsWith(path, ".hyui")) { kind = "U"; }
                    Launch(kind, path);
                } else {
                    Launch("F", path);
                }
            }
            if (handler == "ForgetRecent") {
                int number = System.Convert.ToInt32(Application.Value(args, "recentNumber")) - 1;
                if (Session.RecentAt(number) == "") { Status("Choose a recent entry to remove."); }
                else {
                    Session.Forget(number);
                    Application.SetText("recentList", Session.RecentItems(Application.Value(args, "recentSearch")));
                    Status("Removed from history; your project files were not deleted.");
                }
            }
            if (handler == "CreateProject") {
                string folder = Application.Value(args, Application.Sender(args));
                string name = Application.Value(args, "newName");
                bool ui = Application.Value(args, "newUI") == "true";
                string path = Workspace.Create(folder, name, ui);
                if (path == "") { Status("Choose an empty folder and a valid project name (letters, digits or _)."); }
                else { Launch("P", path); }
            }
            if (handler == "ClonePicked") {
                string url = Application.Value(args, "cloneURL");
                string destination = Application.Value(args, Application.Sender(args));
                if (!Session.GithubURL(url) || destination == "" || Session.Quote(destination) == "") {
                    Status("Enter a valid GitHub HTTPS/SSH repository URL and choose an empty destination folder.");
                    return 0;
                }
                Status("Cloning repository; Git must already be installed.");
                int code = System.Host.Process.Run("git clone " + Session.Quote(url) + " " + Session.Quote(destination));
                if (code != 0) { Status("Git clone failed (exit " + code + "). Check the destination and Git credentials."); }
                else { Launch("F", destination); }
            }
            return 0;
        }

        private static bool EndsWith(string value, string suffix) {
            if (value.Length < suffix.Length) { return false; }
            return Workspace.Slice(value, value.Length - suffix.Length, suffix.Length) == suffix;
        }

        private static void Status(string value) { Application.SetText("startStatus", value); }

        private static void Launch(string kind, string path) {
            if (Session.OpenIDE(kind, path)) {
                Application.Exit();
            } else {
                Status("Cannot launch workspace. Build both applications with scripts/build.sh, then run from the Quark project root.");
            }
        }
    }
}
