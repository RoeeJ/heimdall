defmodule Heimdall.Servers.UDPServer do
  @moduledoc """
  UDP server for receiving DNS queries.
  """
  require Logger
  use GenServer

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def init(opts) do
    port = Keyword.get(opts, :port)
    Logger.info("Heimdall UDP server starting on port #{port}")
    {:ok, socket} = :gen_udp.open(port, [:binary, active: true])
    {:ok, %{socket: socket}}
  end

  def handle_info({:udp, socket, address, port, data}, state) do
    spawn(fn -> handle_packet(socket, address, port, data) end)
    {:noreply, state}
  end

  defp handle_packet(socket, address, port, data) do
    if Heimdall.Servers.Limiter.allow?(address) do
      case Heimdall.DNS.Server.query(data) do
        {:ok, response} ->
          :gen_udp.send(socket, address, port, response)

        {:error, reason} ->
          Logger.error("Failed to handle DNS query: #{reason}")
      end
    else
      Logger.warning("Rate limit exceeded for client #{inspect(address)}")
    end
  end
end
