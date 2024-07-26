defmodule Heimdall.Servers.Tracker do
  use GenServer

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  def init(_state),
    do:
      {:ok,
       %{
         failed_queries: 0,
         blocked_queries: 0,
         successful_queries: 0,
         blocked_clients: %{},
         cache_hits: 0,
         cache_misses: 0
       }}

  def report_failed() do
    GenServer.cast(__MODULE__, {:report, :failed})
  end

  def report_blocked() do
    GenServer.cast(__MODULE__, {:report, :blocked})
  end

  def report_success() do
    GenServer.cast(__MODULE__, {:report, :successful})
  end

  def client_blocked(client_id) do
    GenServer.cast(__MODULE__, {:client_blocked, client_id})
  end

  def client_unblocked(client_id) do
    GenServer.cast(__MODULE__, {:client_unblocked, client_id})
  end

  def get_stats() do
    GenServer.call(__MODULE__, :get_stats)
  end

  def handle_cast({:report, :failed}, state) do
    {:noreply, %{state | failed_queries: state.failed_queries + 1}}
  end

  def handle_cast({:report, :blocked}, state) do
    {:noreply, %{state | blocked_queries: state.blocked_queries + 1}}
  end

  def handle_cast({:report, :successful}, state) do
    {:noreply, %{state | successful_queries: state.successful_queries + 1}}
  end

  def handle_cast({:client_blocked, client_id}, state) do
    {:noreply, %{state | blocked_clients: Map.put(state.blocked_clients, client_id, true)}}
  end

  def handle_cast({:client_unblocked, client_id}, state) do
    {:noreply, %{state | blocked_clients: Map.delete(state.blocked_clients, client_id)}}
  end

  def handle_call(:get_stats, _from, state) do
    {:reply, state, state}
  end
end
