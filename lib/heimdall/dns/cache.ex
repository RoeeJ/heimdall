defmodule Heimdall.DNS.Cache do
  require Logger
  use GenServer
  alias Heimdall.DNS.Resolver

  @entries_table :dns_entries

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  def init(opts) do
    Logger.info("DNS Cache starting")
    entries_table = :ets.new(@entries_table, [:set, :protected, :named_table])
    {:ok, %{entries: entries_table}}
  end

  def handle_call({:query, hostname, record_type}, from, state) do
    case :ets.lookup(@entries_table, {hostname, record_type}) do
      [res] ->
        Logger.info("Cache hit!")
        {:reply, res, state}

      [] ->
        Logger.info("Cache miss!")

        case Resolver.query(hostname, record_type) do
          {:ok, res} ->
            IO.inspect(res)
            # :ets.insert(@entries_table, {{hostname, record_type}, res})
            {:reply, res, state}

          {:error, err} ->
            Logger.error("Error while querying: #{err}")
            {:reply, {:error, err}, state}
        end
    end
  end

  def query(hostname, record_type) do
    Logger.info("Received query request for #{hostname} of record type #{record_type}")
    GenServer.call(__MODULE__, {:query, hostname, record_type})
  end
end
