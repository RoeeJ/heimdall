defmodule Heimdall.DNS.Manager do
  use GenServer
  alias Heimdall.Schema.{Zone, Record, SOA}
  alias Heimdall.Repo
  import Ecto.Query

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @impl true
  def init(_opts) do
    {:ok, %{}}
  end

  # Zone Operations
  def add_zone(zone_params) do
    GenServer.call(__MODULE__, {:add_zone, zone_params})
  end

  def find_zone(query_name) do
    GenServer.call(__MODULE__, {:find_zone, query_name})
  end

  # Record Operations
  def add_record(zone_name, record_params) do
    GenServer.call(__MODULE__, {:add_record, zone_name, record_params})
  end

  def get_records(zone_name, opts \\ []) do
    GenServer.call(__MODULE__, {:get_records, zone_name, opts})
  end

  def update_record(id, params) do
    GenServer.call(__MODULE__, {:update_record, id, params})
  end

  def delete_record(id) do
    GenServer.call(__MODULE__, {:delete_record, id})
  end

  # New method for querying specific subdomain records
  def query_subdomain(full_domain, type \\ nil) do
    GenServer.call(__MODULE__, {:query_subdomain, full_domain, type})
  end

  @impl true
  def handle_call({:add_zone, zone_params}, _from, state) do
    result = create_zone(zone_params)
    {:reply, result, state}
  end

  @impl true
  def handle_call({:find_zone, query_name}, _from, state) do
    result = do_find_zone(query_name)
    {:reply, result, state}
  end

  @impl true
  def handle_call({:add_record, zone_name, record_params}, _from, state) do
    result = do_add_record(zone_name, record_params)
    {:reply, result, state}
  end

  @impl true
  def handle_call({:get_records, zone_name, opts}, _from, state) do
    result = do_get_records(zone_name, opts)
    {:reply, result, state}
  end

  @impl true
  def handle_call({:update_record, id, params}, _from, state) do
    result = do_update_record(id, params)
    {:reply, result, state}
  end

  @impl true
  def handle_call({:delete_record, id}, _from, state) do
    result = do_delete_record(id)
    {:reply, result, state}
  end

  @impl true
  def handle_call({:query_subdomain, full_domain, type}, _from, state) do
    result = do_query_subdomain(full_domain, type)
    {:reply, result, state}
  end

  # Private functions

  defp create_zone(params) do
    %Zone{}
    |> Zone.changeset(params)
    |> Repo.insert()
  end

  defp do_find_zone(query_name) do
    query_parts = String.split(query_name, ".")

    Zone
    |> where(^build_zone_query(query_name, query_parts))
    |> order_by([z], desc: fragment("length(?)", z.name))
    |> limit(1)
    |> Repo.one()
    |> case do
      nil ->
        {:error, :zone_not_found}

      zone ->
        {:ok, zone |> Repo.preload(:soa) |> Repo.preload(:records)}
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

  defp do_add_record(zone_name, record_params) do
    with {:ok, zone} <- do_find_zone(zone_name),
         {:ok, record} <- create_record(zone, record_params) do
      {:ok, record}
    else
      {:error, reason} -> {:error, reason}
    end
  end

  defp create_record(zone, params) do
    %Record{}
    |> Record.changeset(Map.put(params, :zone_id, zone.id))
    |> Repo.insert()
  end

  defp do_get_records(zone_name, opts) do
    with {:ok, zone} <- do_find_zone(zone_name) do
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

  defp do_update_record(id, params) do
    Record
    |> Repo.get(id)
    |> Record.changeset(params)
    |> Repo.update()
  end

  defp do_delete_record(id) do
    Record
    |> Repo.get(id)
    |> Repo.delete()
  end

  defp do_query_subdomain(full_domain, type) do
    parts = String.split(full_domain, ".")

    with {:ok, zone} <- find_matching_zone(parts) do
      subdomain = get_subdomain(parts, zone.name)

      query =
        from r in Record,
          where: r.zone_id == ^zone.id

      query = build_subdomain_query(query, subdomain)
      query = if type, do: where(query, [r], r.type == ^type), else: query

      {:ok, Repo.all(query)}
    end
  end

  defp find_matching_zone(parts) do
    Enum.reduce_while(0..length(parts), {:error, :zone_not_found}, fn i, acc ->
      potential_zone = Enum.take(parts, i * -1) |> Enum.join(".")

      case do_find_zone(potential_zone) do
        {:ok, zone} -> {:halt, {:ok, zone}}
        _ -> {:cont, acc}
      end
    end)
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
end
