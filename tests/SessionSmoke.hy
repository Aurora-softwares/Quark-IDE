using System;
using Quark;

public class Program {
    public static int Main(string[] args) {
        string path = Session.RequestFile();
        System.IO.File.WriteAllText(path, "P\n/tmp/Quark/Quark.IDE.hyproj\n");
        if (Session.Kind() != "P") { return 1; }
        if (Session.Path() != "/tmp/Quark/Quark.IDE.hyproj") { return 2; }
        Session.Clear();
        if (System.IO.File.ReadAllText(path) != "") { return 3; }
        System.IO.File.WriteAllText(path, "E\n\n");
        if (Session.Kind() != "E" || Session.Path() != "") { return 4; }
        Session.Clear();
        System.Console.WriteLine("Quark Hydrogen session handoff tests passed");
        return 0;
    }
}
