defmodule Heimdall.DNS.RecordManager do
  use GenServer
  alias Heimdall.DNS.ZoneManager
  alias Heimdall.Schema.{Zone, Record}
  alias Heimdall.Repo
  import Ecto.Query

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  def init(opts), do: {:ok, %{}}

  def add_record(zone_name, record_params) do
    with {:ok, zone} <- get_zone(zone_name),
         {:ok, record} <- create_record(zone, record_params) do
      {:ok, record}
    else
      {:error, reason} -> {:error, reason}
    end
  end

  def get_records(zone_name, opts \\ []) do
    with {:ok, zone} <- get_zone(zone_name) do
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
    else
      {:error, reason} -> {:error, reason}
    end
  end

  def update_record(id, params) do
    Record
    |> Repo.get(id)
    |> Record.changeset(params)
    |> Repo.update()
  end

  def delete_record(id) do
    Record
    |> Repo.get(id)
    |> Repo.delete()
  end

  defp get_zone(name) do

    case ZoneManager.find_zone(name) do
      nil -> {:error, :zone_not_found}
      {:ok, zone} -> {:ok, zone}
    end
  end

  defp create_record(zone, params) do
    %Record{}
    |> Record.changeset(Map.put(params, :zone_id, zone.id))
    |> Repo.insert()
  end
end
