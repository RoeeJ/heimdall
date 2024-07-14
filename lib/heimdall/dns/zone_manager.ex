defmodule Heimdall.DNS.ZoneManager do
  use GenServer
  alias Heimdall.Schema.{Zone, SOA, Record}
  alias Heimdall.DNS.RecordManager
  alias Heimdall.Repo
  import Ecto.Query

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @impl true
  def init(_opts) do
    {:ok, %{}}
  end

  def add_zone(zone_params) do
    GenServer.call(__MODULE__, {:add_zone, zone_params})
  end

  @spec find_zone(String.t()) :: Zone.t() | nil
  def find_zone(query_name) do
    GenServer.call(__MODULE__, {:find_zone, query_name})
  end

  @impl true
  def handle_call({:add_zone, zone_params}, _from, state) do
    case create_zone(zone_params) do
      {:ok, zone} -> {:reply, {:ok, zone}, state}
      {:error, changeset} -> {:reply, {:error, changeset}, state}
    end
  end

  @impl true
  def handle_call({:find_zone, query_name}, _from, state) do
    query_parts = String.split(query_name, ".")

    Zone
    |> where(^build_zone_query(query_name, query_parts))
    |> order_by([z], desc: fragment("length(?)", z.name))
    |> limit(1)
    |> Repo.one()
    |> case do
      nil ->
        {:reply, {:error, "No matching zone found for #{query_name}"}, state}

      zone ->
        zone =
          zone
          |> Repo.preload(:soa)
          |> Repo.preload(:records)

        {:reply, {:ok, zone}, state}
    end
  end

  defp create_zone(params) do
    %Zone{}
    |> Zone.changeset(params)
    |> Repo.insert()
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
end
