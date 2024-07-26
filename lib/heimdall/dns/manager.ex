defmodule Heimdall.DNS.Manager do
  @moduledoc """
  Module for managing DNS zones and records.
  """
  alias Heimdall.Schema.{Zone, Record}
  alias Heimdall.Repo
  import Ecto.Query

  def add_zone(zone_params) do
    %Zone{}
    |> Zone.changeset(zone_params)
    |> Repo.insert()
  end

  def find_zone(query_name) do
    query_parts = String.split(query_name, ".")

    Zone
    |> where(^build_zone_query(query_name, query_parts))
    |> order_by([z], desc: fragment("length(?)", z.name))
    |> limit(1)
    |> Repo.one()
    |> case do
      nil -> {:error, :zone_not_found}
      zone -> {:ok, zone |> Repo.preload(:soa) |> Repo.preload(:records)}
    end
  end

  def get_zone(zone_id) do
    case Zone
         |> Repo.get(zone_id)
         |> Repo.preload(records: from(r in Record, order_by: r.id)) do
      nil -> {:error, :zone_not_found}
      zone -> {:ok, zone}
    end
  end

  def add_record(zone_id, record_params) do
    Repo.transaction(fn ->
      with {:ok, zone} <- get_zone(zone_id),
           {:ok, record} <-
             %Record{}
             |> Record.changeset(Map.put(record_params, :zone_id, zone.id))
             |> Repo.insert(),
           {:ok, _updated_zone} <- increment_zone_serial(zone),
           cache_key <- generate_cache_key(record, zone) do
        Heimdall.DNS.Cache.delete(cache_key)
        {:ok, record}
      else
        {:error, reason} -> Repo.rollback(reason)
      end
    end)
  end

  def get_records(zone_name, opts \\ []) do
    with {:ok, zone} <- find_zone(zone_name) do
      query = from r in Record, where: r.zone_id == ^zone.id

      query =
        case Keyword.get(opts, :type) do
          nil -> query
          type -> where(query, [r], r.type == ^type)
        end

      query =
        case Keyword.get(opts, :name) do
          nil -> query
          "@" -> where(query, [r], r.name == "")
          name -> where(query, [r], r.name == ^name)
        end

      {:ok, Repo.all(query)}
    end
  end

  def update_record(id, params) do
    Repo.transaction(fn ->
      record = Repo.get!(Record, id)
      changeset = Record.changeset(record, params)

      if changeset.changes == %{} do
        {:ok, record}
      else
        with {:ok, updated_record} <- Repo.update(changeset),
             zone <- Repo.get!(Zone, updated_record.zone_id),
             {:ok, _updated_zone} <- increment_zone_serial(zone) do
          # Clear cache for relevant records
          cache_key = generate_cache_key(updated_record, zone)
          Heimdall.DNS.Cache.delete(cache_key)
          {:ok, updated_record}
        else
          {:error, reason} -> Repo.rollback(reason)
        end
      end
    end)
  end

  def delete_record(id) do
    Repo.transaction(fn ->
      with {:ok, record} <- Repo.get(Record, id) |> Repo.delete(),
           {:ok, zone} <- get_zone(record.zone_id),
           {:ok, _updated_zone} <- increment_zone_serial(zone),
           cache_key <- generate_cache_key(record, zone) do
        Heimdall.DNS.Cache.delete(cache_key)
        {:ok, record}
      else
        {:error, reason} -> Repo.rollback(reason)
      end
    end)
  end

  def query_subdomain(full_domain, type \\ nil) do
    parts = String.split(full_domain, ".")
    parts_length = length(parts)

    Enum.reduce_while(1..(parts_length - 1), {:error, :nxdomain}, fn i, acc ->
      potential_zone = Enum.take(parts, -i) |> Enum.join(".")

      case find_zone(potential_zone) do
        {:ok, zone} -> {:halt, {:ok, zone}}
        _ -> {:cont, acc}
      end
    end)
    |> case do
      {:ok, zone} ->
        subdomain = get_subdomain(parts, zone.name)

        query =
          from r in Record,
            where: r.zone_id == ^zone.id

        query = build_subdomain_query(query, subdomain)

        query =
          case type do
            nil -> query
            :any -> query
            _ -> where(query, [r], r.type == ^type)
          end

        records = Repo.all(query)

        records =
          case type do
            :any -> records
            _ -> Enum.filter(records, fn r -> r.type == type end)
          end

        {:ok, records}

      _ ->
        # Check for 1 part TLD
        case find_zone(full_domain) do
          {:ok, zone} ->
            subdomain = get_subdomain(parts, zone.name)

            query =
              from r in Record,
                where: r.zone_id == ^zone.id

            query = build_subdomain_query(query, subdomain)

            query =
              case type do
                nil -> query
                :any -> query
                _ -> where(query, [r], r.type == ^type)
              end

            records = Repo.all(query)

            records =
              case type do
                :any -> records
                _ -> Enum.filter(records, fn r -> r.type == type end)
              end

            {:ok, records}

          _ ->
            {:error, :nxdomain}
        end
    end
  end

  def stats do
    %{
      total_queries: 0,
      failed_queries: 0,
      blocked_queries: 0,
      rate_limit_blocked_clients: 0,
      cache_stats: %{},
      recent_queries: []
    }
  end

  defp generate_cache_key(updated_record, zone) do
    case updated_record.name do
      "@" -> zone.name
      _ -> "#{updated_record.name}.#{zone.name}"
    end
  end

  defp build_zone_query(query_name, query_parts) do
    exact_match = dynamic([z], z.name == ^query_name)

    subdomain_matches =
      Enum.reduce(1..length(query_parts), exact_match, fn i, acc ->
        subset = Enum.take(query_parts, i * -1)
        potential_zone = Enum.join(subset, ".")
        dynamic([z], ^acc or z.name == ^potential_zone)
      end)

    subdomain_matches
  end

  defp get_subdomain(full_parts, zone_parts) do
    zone_length = length(String.split(zone_parts, "."))

    Enum.take(full_parts, length(full_parts) - zone_length)
    |> Enum.join(".")
  end

  defp build_subdomain_query(query, "") do
    # Root domain query - match only @ or empty string
    where(query, [r], r.name == "@" or r.name == "")
  end

  defp build_subdomain_query(query, subdomain) do
    # Subdomain query - match the exact subdomain and all records under it
    where(query, [r], r.name == ^subdomain)
  end

  defp increment_zone_serial(zone) do
    updated_zone = Zone.changeset(zone, %{serial: zone.serial + 1})

    case Repo.update(updated_zone) do
      {:ok, zone} -> {:ok, zone}
      {:error, changeset} -> {:error, changeset}
    end
  end
end
