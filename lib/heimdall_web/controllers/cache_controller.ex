defmodule HeimdallWeb.CacheController do
  alias Heimdall.DNS.Cache
  use HeimdallWeb, :controller

  def clear(conn, _params) do
    case Cache.clear() do
      {:ok, sz} ->
        conn
        |> put_resp_header("X-Cache-Size", Integer.to_string(sz))
        |> json(%{status: "ok", size: sz})

      {:error, _} ->
        conn
        |> json(%{status: "error"})
    end
  end

  def stats(conn, _params) do
    case Cache.stats() do
      {:ok, stats} ->
        conn
        |> put_resp_header("X-Cache-Count", to_string(stats.count))
        |> put_resp_header("X-Cache-Size", to_string(stats.size))
        |> put_resp_header("X-Cache-Hits", to_string(stats.hits))
        |> put_resp_header("X-Cache-Evictions", to_string(stats.evictions))
        |> put_resp_header("X-Cache-Writes", to_string(stats.writes))
        |> put_resp_header("X-Cache-Expirations", to_string(stats.expirations))
        |> put_resp_header("X-Cache-Misses", to_string(stats.misses))
        |> put_resp_header("X-Cache-Hit-Rate", to_string(stats.hit_rate))
        |> json(Map.merge(stats, %{status: "ok"}))

      _ ->
        conn
        |> put_resp_header("X-Error-Reason", "Failed to get cache stats")
        |> json(%{status: "error"})
    end
  end
end
