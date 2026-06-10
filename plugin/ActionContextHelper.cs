using System;
using System.Collections.Generic;
using System.Reflection;
using Quicker.Public.Interfaces;

namespace VoxType.Plugin;

internal static class ActionContextHelper
{
    public const string OutputTextVar = "voxtype_text";
    public const string OutputStatusVar = "voxtype_status";
    public const string OutputInstallerPathVar = "voxtype_installer_path";
    public const string OutputErrorVar = "voxtype_error";

    public static string? TryGetQuickerInParam(IActionContext? context)
    {
        if (context is null)
        {
            return null;
        }

        foreach (var ctx in EnumerateContextChain(context))
        {
            try
            {
                var value = ctx.GetVarValue("quicker_in_param");
                if (value is string text && !string.IsNullOrWhiteSpace(text))
                {
                    return text;
                }

                if (value is not null && !string.IsNullOrWhiteSpace(value.ToString()))
                {
                    return value.ToString();
                }
            }
            catch
            {
                // Variable may be missing.
            }

            var fromProperty = TryReadStringProperty(ctx, "InputParam", "QuickerInParam", "InParam");
            if (!string.IsNullOrWhiteSpace(fromProperty))
            {
                return fromProperty;
            }
        }

        return null;
    }

    public static void TrySetVar(IActionContext? context, string name, string? value)
    {
        if (context is null || string.IsNullOrWhiteSpace(name))
        {
            return;
        }

        foreach (var ctx in EnumerateContextChain(context))
        {
            try
            {
                ctx.SetVarValue(name, value ?? string.Empty);
                return;
            }
            catch
            {
                // Ignore and try parent/root.
            }
        }
    }

    private static IEnumerable<IActionContext> EnumerateContextChain(IActionContext context)
    {
        var visited = new HashSet<object>(ReferenceEqualityComparer.Instance);
        IActionContext? current = context;
        while (current is not null && visited.Add(current))
        {
            yield return current;

            IActionContext? root = null;
            try
            {
                root = current.GetRootContext();
            }
            catch
            {
                // Ignore.
            }

            if (root is not null && !ReferenceEquals(root, current) && visited.Add(root))
            {
                yield return root;
            }

            IActionContext? parent = null;
            try
            {
                parent = current.GetParentContext();
            }
            catch
            {
                // Ignore.
            }

            current = parent;
        }
    }

    private static string? TryReadStringProperty(object instance, params string[] propertyNames)
    {
        foreach (var propertyName in propertyNames)
        {
            try
            {
                var property = instance.GetType().GetProperty(
                    propertyName,
                    BindingFlags.Public | BindingFlags.Instance);
                if (property?.GetValue(instance) is string text && !string.IsNullOrWhiteSpace(text))
                {
                    return text;
                }
            }
            catch
            {
                // Ignore reflection failures.
            }
        }

        return null;
    }

    private sealed class ReferenceEqualityComparer : IEqualityComparer<object>
    {
        public static ReferenceEqualityComparer Instance { get; } = new();

        public new bool Equals(object? x, object? y) => ReferenceEquals(x, y);

        public int GetHashCode(object obj) =>
            System.Runtime.CompilerServices.RuntimeHelpers.GetHashCode(obj);
    }
}
