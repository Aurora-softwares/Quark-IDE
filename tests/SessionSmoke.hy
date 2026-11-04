using System;
using Quark;

public class Program {
    public static int Main(string[] args) {
        System.Console.WriteLine("SESSION_STEP 0 entry");
        string path = Session.RequestFile();
        System.Console.WriteLine("SESSION_STEP 1 request path: " + path);
        System.IO.File.WriteAllText(path, "P\n/tmp/Quark/Quark.IDE.hyproj\n");
        System.Console.WriteLine("SESSION_STEP 2 request written");
        string kind = Session.Kind();
        System.Console.WriteLine("SESSION_STEP 3 kind read: " + kind);
        if (kind != "P") { return 1; }
        string selected = Session.Path();
        System.Console.WriteLine("SESSION_STEP 4 session path read: " + selected);
        if (selected != "/tmp/Quark/Quark.IDE.hyproj") { return 2; }
        System.Console.WriteLine("SESSION_STEP 5 about to clear");
        Session.Clear();
        System.Console.WriteLine("SESSION_STEP 6 cleared");
        if (System.IO.File.ReadAllText(path) != "") { return 3; }
        System.IO.File.WriteAllText(path, "E\n\n");
        System.Console.WriteLine("SESSION_STEP 7 empty request written");
        if (Session.Kind() != "E" || Session.Path() != "") { return 4; }
        Session.Clear();
        System.Console.WriteLine("Quark Hydrogen session handoff tests passed");
        return 0;
    }
}
