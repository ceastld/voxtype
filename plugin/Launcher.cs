using System;
using System.Diagnostics;
using System.IO;
using System.Net.Http;
using System.Threading.Tasks;
using Quicker.Public.Interfaces;

namespace VoxType.Plugin;

/// <summary>
/// Quicker plugin entry — trigger VoxType dictation via local HTTP API.
/// </summary>
public static class Launcher
{
    private const string ApiBase = "http://127.0.0.1:6020";
    private static readonly HttpClient Http = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };

    /// <summary>
    /// Toggle dictation (start if idle, stop and type if recording).
    /// </summary>
    public static void Start(IActionContext? context = null)
    {
        _ = StartAsync();
    }

    /// <summary>
    /// Hold-to-talk mode: call StartDictation on press and StopDictation on release from Quicker.
    /// </summary>
    public static void StartDictation(IActionContext? context = null)
    {
        _ = PostAsync("/dictate/start");
    }

    public static void StopDictation(IActionContext? context = null)
    {
        _ = PostAsync("/dictate/stop");
    }

    private static async Task StartAsync()
    {
        try
        {
            await EnsureClientRunningAsync().ConfigureAwait(false);
            await PostAsync("/dictate/toggle").ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[VoxType.Plugin] {ex}");
        }
    }

    private static async Task EnsureClientRunningAsync()
    {
        try
        {
            var health = await Http.GetAsync($"{ApiBase}/health").ConfigureAwait(false);
            if (health.IsSuccessStatusCode) return;
        }
        catch
        {
            // fall through to launch
        }

        var exe = ResolveClientExe();
        if (!File.Exists(exe))
            throw new FileNotFoundException("VoxType client not found", exe);

        Process.Start(new ProcessStartInfo
        {
            FileName = exe,
            UseShellExecute = true,
            WorkingDirectory = Path.GetDirectoryName(exe) ?? "",
        });

        for (var i = 0; i < 40; i++)
        {
            await Task.Delay(500).ConfigureAwait(false);
            try
            {
                var health = await Http.GetAsync($"{ApiBase}/health").ConfigureAwait(false);
                if (health.IsSuccessStatusCode) return;
            }
            catch
            {
                // retry
            }
        }

        throw new TimeoutException("VoxType client did not become ready");
    }

    private static string ResolveClientExe()
    {
        var env = Environment.GetEnvironmentVariable("VOXTYPE_CLIENT_EXE");
        if (!string.IsNullOrWhiteSpace(env)) return env;

        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var installed = Path.Combine(local, "Programs", "VoxType", "VoxType.exe");
        if (File.Exists(installed)) return installed;

        return Path.Combine(local, "Programs", "VoxType", "VoxType.exe");
    }

    private static async Task PostAsync(string path)
    {
        await EnsureClientRunningAsync().ConfigureAwait(false);
        var response = await Http.PostAsync($"{ApiBase}{path}", null).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
    }
}
