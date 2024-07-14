defmodule HeimdallWeb.CacheController do
  alias Heimdall.DNS.Cache
  use HeimdallWeb, :controller

  def clear(conn, params) do
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

  def stats(conn, params) do
    conn |> send_resp(:ok, "")
  end
end
