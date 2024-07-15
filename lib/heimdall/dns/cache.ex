defmodule Heimdall.DNS.Cache do
  @cache_name :dns_cache

  def get(key) do
    case Cachex.get(@cache_name, key) do
      {:ok, nil} ->
        {:ok, nil}

      {:ok, {last_fetch_time, resources}} ->
        now = System.system_time(:second)

        {valid_resources, expired_resources} =
          Enum.split_with(resources, fn {expiration, _} ->
            expiration > now
          end)

        updated_resources =
          Enum.map(valid_resources, fn {expiration, resource} ->
            %{resource | ttl: expiration - now}
          end)

        cond do
          Enum.empty?(valid_resources) ->
            Cachex.del(@cache_name, key)
            {:ok, nil}

          Enum.empty?(expired_resources) ->
            {:ok, updated_resources}

          true ->
            {:partial, updated_resources, last_fetch_time}
        end

      error ->
        error
    end
  end

  def put(key, resources) do
    now = System.system_time(:second)

    cached_resources =
      Enum.map(resources, fn resource ->
        expiration = now + resource.ttl
        {expiration, resource}
      end)

    max_ttl =
      Enum.max_by(cached_resources, fn {expiration, _} -> expiration end)
      |> elem(0)
      |> Kernel.-(now)

    Cachex.put(@cache_name, key, {now, cached_resources}, ttl: :timer.seconds(max_ttl))
  end

  def stats() do
    with {:ok, count} <- Cachex.count(@cache_name),
         {:ok, size} <- Cachex.size(@cache_name) do
      %{
        count: count,
        size: size
      }
    else
      _ -> %{}
    end
  end

  def clear() do
    Cachex.clear(@cache_name)
  end
end
