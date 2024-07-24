defmodule Heimdall.Servers.Limiter do
  use GenServer

  @rate_limit 1000 # default to 1000 requests per minute00

  def start_link(_) do
    GenServer.start_link(__MODULE__, %{}, name: __MODULE__)
  end

  def init(state) do
    {:ok, state}
  end

  def allow?(socket) do
    GenServer.call(__MODULE__, {:allow?, socket})
  end

  def handle_call({:allow?, socket}, _from, state) do
    current_time = :os.system_time(:second)
    {allowed, new_state} = check_rate_limit(socket, current_time, state)
    {:reply, allowed, new_state}
  end

  defp check_rate_limit(socket, current_time, state) do
    socket_key = inspect(socket)
    socket_state = Map.get(state, socket_key, {0, current_time})

    {request_count, last_request_time} = socket_state

    if current_time - last_request_time > 60 do
      # Reset the count if more than a minute has passed
      {true, Map.put(state, socket_key, {1, current_time})}
    else
      if request_count < @rate_limit do
        {true, Map.put(state, socket_key, {request_count + 1, last_request_time})}
      else
        {false, state}
      end
    end
  end
end
