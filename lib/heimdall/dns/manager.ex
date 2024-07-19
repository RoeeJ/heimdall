defmodule Heimdall.DNS.Manager do
  @moduledoc """
  Module for managing DNS zones and records.
  """
  alias Heimdall.DNS.Model
  alias Heimdall.Schema.{Zone, Record}
  alias Heimdall.Repo
  import Ecto.Query

  # Zone Operations
  @spec add_zone(map()) :: {:ok, Zone.t()} | {:error, Ecto.Changeset.t()}
  def add_zone(zone_params) do
    %Zone{}
    |> Zone.changeset(zone_params)
    |> Repo.insert()
  end

  @spec find_zone(String.t()) :: {:ok, Zone.t()} | {:error, :zone_not_found}
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

  @spec get_zone(integer()) :: {:ok, Zone.t()} | {:error, :zone_not_found}
  def get_zone(zone_id) do
    case Zone |> Repo.get(zone_id) |> Repo.preload(records: from(r in Record, order_by: r.id)) do
      nil -> {:error, :zone_not_found}
      zone -> {:ok, zone}
    end
  end

  # Record Operations
  @spec add_record(String.t(), map()) :: {:ok, Record.t()} | {:error, any()}
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

  @spec get_records(String.t(), Keyword.t()) :: {:ok, [Record.t()]} | {:error, :zone_not_found}
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

  @spec update_record(integer(), map()) :: {:ok, Record.t()} | {:error, any()}
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

  @spec delete_record(integer()) :: {:ok, Record.t()} | {:error, any()}
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

  # New method for querying specific subdomain records
  @spec query_subdomain(String.t(), Model.qtype_atoms() | nil) ::
          {:ok, [Record.t()]} | {:error, :nxdomain}
  def query_subdomain(full_domain, type \\ nil)
  def query_subdomain(".", _type), do: {:error, :nxdomain}

  def query_subdomain(full_domain, type) do
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
        query = if type, do: where(query, [r], r.type == ^type), else: query

        records = Repo.all(query)

        if records == [] do
          {:error, :nxdomain}
        else
          {:ok, records}
        end

      _ ->
        {:error, :nxdomain}
    end
  end

  defp generate_cache_key(updated_record, zone) do
    case updated_record.name do
      "@" -> {zone.name, updated_record.type}
      _ -> {"#{updated_record.name}.#{zone.name}", updated_record.type}
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
