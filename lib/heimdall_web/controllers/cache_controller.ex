defmodule HeimdallWeb.CacheController do
  alias Heimdall.DNS.Cache
  use HeimdallWeb, :controller

  def clear(conn, params) do
    case Cache.clear() do
      {:ok, sz} -> conn |> put_resp_header("X-Cache-Size", Integer.to_string(sz)) |> send_resp(:ok, "OK")
      {:error, reason} -> conn |> put_resp_header("X-Error-Reason", reason) |> send_resp(:server_error, reason)
    end
  end

  def stats(conn, params) do
    case Cache.stats() do
      %{count: cache_count, size: cache_size} ->
        conn
        |> put_resp_header("X-Cache-Count", to_string(cache_count))
        |> put_resp_header("X-Cache-Size", to_string(cache_size))
        |> resp(:ok, "")

      r ->
        conn
    end
  end
end
