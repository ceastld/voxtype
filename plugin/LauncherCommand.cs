using System;

namespace VoxType.Plugin;

public enum VoxTypeCommand
{
    Toggle,
    Start,
    Stop,
    Ensure,
    Status,
    DownloadInstaller,
}

/// <summary>
/// Parses <c>quicker_in_param</c> from <c>quicker:runaction</c> into a dictation command.
/// </summary>
public static class LauncherCommandParser
{
    public static VoxTypeCommand Parse(string? quickerInParam)
    {
        var mode = Normalize(quickerInParam);
        if (string.IsNullOrEmpty(mode))
        {
            return VoxTypeCommand.Toggle;
        }

        if (mode.Equals("start", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("dictate-start", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("record-start", StringComparison.OrdinalIgnoreCase))
        {
            return VoxTypeCommand.Start;
        }

        if (mode.Equals("stop", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("dictate-stop", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("record-stop", StringComparison.OrdinalIgnoreCase))
        {
            return VoxTypeCommand.Stop;
        }

        if (mode.Equals("toggle", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("dictate-toggle", StringComparison.OrdinalIgnoreCase))
        {
            return VoxTypeCommand.Toggle;
        }

        if (mode.Equals("ensure", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("launch", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("boot", StringComparison.OrdinalIgnoreCase))
        {
            return VoxTypeCommand.Ensure;
        }

        if (mode.Equals("status", StringComparison.OrdinalIgnoreCase))
        {
            return VoxTypeCommand.Status;
        }

        if (mode.Equals("download", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("install", StringComparison.OrdinalIgnoreCase)
            || mode.Equals("download-installer", StringComparison.OrdinalIgnoreCase))
        {
            return VoxTypeCommand.DownloadInstaller;
        }

        return VoxTypeCommand.Toggle;
    }

    private static string Normalize(string? quickerInParam)
    {
        var mode = (quickerInParam ?? string.Empty).Trim();
        if (mode.StartsWith("?", StringComparison.Ordinal))
        {
            mode = mode.Substring(1).Trim();
        }

        return mode;
    }
}
