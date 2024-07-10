defmodule Heimdall.DNS.Resolver do
  require Logger
  use GenServer

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  def init(opts) do
    {:ok, opts}
  end

  def handle_call({:resolve, hostname, record_type}, from, state) do
    case DNS.resolve(hostname, record_type, nameservers: [{"8.8.4.4", 53}]) do
      {:ok, res} ->
        {:reply, {:ok, res}, state}

      {:error, err} ->
        {:reply, {:error, err}, state}
    end
  end

  def handle_call({:query, hostname, record_type}, from, state) do
    Logger.info("hostname: #{hostname}, record_type: #{record_type}")

    case DNS.query(hostname, record_type, nameservers: [{"8.8.4.4", 53}]) do
      {:ok, res} ->
        {:reply, {:ok, res}, state}

      {:error, err} ->
        {:reply, {:error, err}, state}
    end
  end

  def resolve(hostname, record_type),
    do: GenServer.call(__MODULE__, {:resolve, hostname, record_type})

  def query(hostname, record_type),
    do: GenServer.call(__MODULE__, {:query, hostname, record_type})
end
