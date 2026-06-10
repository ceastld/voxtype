using System;
using System.Diagnostics;
using System.Threading.Tasks;
using Quicker.Public.Interfaces;

namespace VoxType.Plugin;

/// <summary>
/// Quicker plugin entry — control VoxType dictation via local HTTP API (127.0.0.1:6020).
/// </summary>
public static class Launcher
{
    /// <summary>
    /// Default: toggle dictation (start if idle, stop and type if recording).
    /// </summary>
    public static void Start(IActionContext? context = null) =>
        Execute(LauncherCommandParser.Parse(ActionContextHelper.TryGetQuickerInParam(context)), context);

    /// <summary>
    /// Explicit <c>quicker_in_param</c> from <c>quicker:runaction:{id}?start</c> etc.
    /// </summary>
    public static void StartFromQuickerInParam(string? quickerInParam, IActionContext? context = null) =>
        Execute(LauncherCommandParser.Parse(quickerInParam), context);

    /// <summary>Hold-to-talk: press.</summary>
    public static void StartDictation(IActionContext? context = null) =>
        Execute(VoxTypeCommand.Start, context);

    /// <summary>Hold-to-talk: release — waits for transcription and sets <c>voxtype_text</c>.</summary>
    public static void StopDictation(IActionContext? context = null) =>
        Execute(VoxTypeCommand.Stop, context);

    /// <summary>Download NSIS installer to Downloads, then launch it (UAC prompt).</summary>
    public static void DownloadInstaller(IActionContext? context = null) =>
        Execute(VoxTypeCommand.DownloadInstaller, context);

    /// <summary>Launch VoxType.exe if API is not reachable.</summary>
    public static void EnsureClient(IActionContext? context = null) =>
        Execute(VoxTypeCommand.Ensure, context);

    private static void Execute(VoxTypeCommand command, IActionContext? context)
    {
        try
        {
            RunAsync(command, context).GetAwaiter().GetResult();
            ActionContextHelper.TrySetVar(context, ActionContextHelper.OutputErrorVar, string.Empty);
        }
        catch (Exception ex)
        {
            ActionContextHelper.TrySetVar(context, ActionContextHelper.OutputErrorVar, ex.Message);
            Debug.WriteLine($"[VoxType.Plugin] {ex}");
            throw;
        }
    }

    private static async Task RunAsync(VoxTypeCommand command, IActionContext? context)
    {
        var client = new VoxTypeClient();
        switch (command)
        {
            case VoxTypeCommand.Ensure:
                await client.EnsureRunningAsync().ConfigureAwait(false);
                break;

            case VoxTypeCommand.Start:
                await client.EnsureRunningAsync().ConfigureAwait(false);
                await client.StartAsync().ConfigureAwait(false);
                break;

            case VoxTypeCommand.Stop:
            {
                var text = await client.StopAsync().ConfigureAwait(false);
                ActionContextHelper.TrySetVar(context, ActionContextHelper.OutputTextVar, text);
                break;
            }

            case VoxTypeCommand.Toggle:
                await client.EnsureRunningAsync().ConfigureAwait(false);
                await client.ToggleAsync().ConfigureAwait(false);
                break;

            case VoxTypeCommand.Status:
            {
                var statusJson = await client.GetStatusJsonAsync().ConfigureAwait(false);
                ActionContextHelper.TrySetVar(context, ActionContextHelper.OutputStatusVar, statusJson);
                break;
            }

            case VoxTypeCommand.DownloadInstaller:
            {
                var path = await InstallerDownload.DownloadLatestAsync().ConfigureAwait(false);
                ActionContextHelper.TrySetVar(context, ActionContextHelper.OutputInstallerPathVar, path);
                InstallerDownload.LaunchInstaller(path);
                break;
            }

            default:
                throw new ArgumentOutOfRangeException(nameof(command), command, null);
        }
    }
}
