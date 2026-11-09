using System;
using System.IO;
using Quark;

public class Program {
    public static int Main(string[] args) {
        if (args.Length != 3) { return 1; }
        string project = Workspace.Join(args[1], "App.hyproj");
        string source = Workspace.Join(args[1], "App.hy");
        System.IO.File.WriteAllText(source,
            "using System; public class Program { public static int Main(string[] args) { System.Console.WriteLine(\"Quark arm64 run ok\"); return 0; } }");
        System.IO.File.WriteAllText(project,
            "format = 2\nname = \"QuarkArmApp\"\ntype = \"exe\"\nsources = [\"App.hy\"]\n");
        string built = BuildService.Execute(args[0], project, "Build", false, "macos-arm64");
        if (Workspace.FindAt(built, "Build completed.", 0) < 0 ||
            !System.IO.File.Exists(project + ".quark-build")) { return 2; }
        string ran = BuildService.Execute(args[0], project, "Run", false, "macos-arm64");
        if (Workspace.FindAt(ran, "Quark arm64 run ok", 0) < 0 ||
            Workspace.FindAt(ran, "Exit code: 0", 0) < 0) { return 3; }
        string uiBuilt = BuildService.Execute(args[0], args[2], "Build", true, "macos-arm64");
        string bundle = args[2] + ".quark-build.app";
        if (Workspace.FindAt(uiBuilt, "Build completed.", 0) < 0 ||
            !System.IO.File.Exists(bundle + "/Contents/MacOS/hyui-launcher")) { return 4; }
        string uiRan = BuildService.Execute(args[0], args[2], "Run", true, "macos-arm64");
        if (Workspace.FindAt(uiRan, "Launch result:", 0) < 0 ||
            Workspace.FindAt(uiRan, "Exit code: 0", 0) < 0) { return 5; }
        System.Console.WriteLine("Quark Apple Silicon console and AppKit Build/Run passed");
        return 0;
    }
}
