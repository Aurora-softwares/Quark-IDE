using System;
using System.IO;
using Quark;

public class Program {
    public static int Main(string[] args) {
        if (args.Length != 1) { return 1; }
        string project = Workspace.Join(args[0], "Target.hyproj");
        System.IO.File.WriteAllText(project, "format = 2\nname = \"Target\"\ntype = \"exe\"\n");
        string built = BuildService.Execute("/bin/echo", project, "Build", false, "host");
        if (Workspace.FindAt(built, "Build completed.", 0) < 0 ||
            Workspace.FindAt(built, " -o ", 0) < 0 ||
            Workspace.FindAt(built, "--target macos-arm64", 0) >= 0) { return 2; }
        string invalid = BuildService.Execute("/bin/echo", project, "Build", false, "bad-target");
        if (invalid != "Unknown application build target.") { return 3; }
        string unavailable = BuildService.Execute("/bin/echo", project, "Build", false, "macos-arm64");
        if (unavailable != "macOS arm64 builds require macOS and Xcode Command Line Tools.") { return 4; }
        System.Console.WriteLine("Quark target selection and host build tests passed");
        return 0;
    }
}
