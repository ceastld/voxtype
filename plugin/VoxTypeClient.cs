using System;
using System.Diagnostics;
using System.IO;
using System.Net.Http;
using System.Threading;
using System.Threading.Tasks;

namespace VoxType.Plugin;

internal sealed class VoxTypeClient
{
    private static readonly HttpClient Http = new HttpClient
    {
        Timeout = TimeSpan.FromSeconds(60),
    };

    private readonly string _apiBase;

    public VoxTypeClient()
    {
        var port = Environment.GetEnvironmentVariable("VOXTYPE_API_PORT");
        if (!string.IsNullOrWhiteSpace(port) && ushort.TryParse(port, out var parsed))
        {
            _apiBase = $"http://127.0.0.1:{parsed}";
        }
        else
        {
            _apiBase = "http://127.0.0.1:6020";
        }
    }

    public async Task EnsureRunningAsync(CancellationToken cancellationToken = default)
    {
        if (await TryHealthAsync(cancellationToken).ConfigureAwait(false))
        {
            return;
        }

        var exe = ResolveClientExe();
        if (!File.Exists(exe))
        {
            throw new FileNotFoundException(
                "未找到 VoxType 客户端，请先安装 VoxType 或执行 download 命令下载安装包",
                exe);
        }

        Process.Start(new ProcessStartInfo
        {
            FileName = exe,
            UseShellExecute = true,
            WorkingDirectory = Path.GetDirectoryName(exe) ?? "",
        });

        for (var i = 0; i < 40; i++)
        {
            cancellationToken.ThrowIfCancellationRequested();
            await Task.Delay(500, cancellationToken).ConfigureAwait(false);
            if (await TryHealthAsync(cancellationToken).ConfigureAwait(false))
            {
                return;
            }
        }

        throw new TimeoutException("VoxType 客户端启动超时，请检查是否被安全软件拦截");
    }

    public Task StartAsync(CancellationToken cancellationToken = default) =>
        PostAsync("/dictate/start", cancellationToken);

    public async Task<string> StopAsync(CancellationToken cancellationToken = default)
    {
        var json = await PostRawAsync("/dictate/stop", cancellationToken).ConfigureAwait(false);
        return JsonResponseReader.ReadString(json, "text") ?? string.Empty;
    }

    public Task ToggleAsync(CancellationToken cancellationToken = default) =>
        PostAsync("/dictate/toggle", cancellationToken);

    public async Task<string> GetStatusJsonAsync(CancellationToken cancellationToken = default)
    {
        await EnsureRunningAsync(cancellationToken).ConfigureAwait(false);
        using var response = await Http.GetAsync($"{_apiBase}/status", cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        return await response.Content.ReadAsStringAsync().ConfigureAwait(false);
    }

    private async Task PostAsync(string path, CancellationToken cancellationToken)
    {
        await EnsureRunningAsync(cancellationToken).ConfigureAwait(false);
        using var response = await Http.PostAsync($"{_apiBase}{path}", null, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
    }

    private async Task<string> PostRawAsync(string path, CancellationToken cancellationToken)
    {
        await EnsureRunningAsync(cancellationToken).ConfigureAwait(false);
        using var response = await Http.PostAsync($"{_apiBase}{path}", null, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        return await response.Content.ReadAsStringAsync().ConfigureAwait(false);
    }

    private async Task<bool> TryHealthAsync(CancellationToken cancellationToken)
    {
        try
        {
            using var response = await Http.GetAsync($"{_apiBase}/health", cancellationToken)
                .ConfigureAwait(false);
            return response.IsSuccessStatusCode;
        }
        catch
        {
            return false;
        }
    }

    public static string ResolveClientExe()
    {
        var env = Environment.GetEnvironmentVariable("VOXTYPE_CLIENT_EXE");
        if (!string.IsNullOrWhiteSpace(env))
        {
            return env;
        }

        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        return Path.Combine(local, "VoxType", "VoxType.exe");
    }

    public static bool IsClientInstalled() => File.Exists(ResolveClientExe());
}
