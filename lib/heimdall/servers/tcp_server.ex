defmodule Heimdall.Servers.TCPServer do
  @moduledoc """
  TCP server for receiving DNS queries.
  """
  require Logger
  use GenServer

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def init(opts) do
    port = Keyword.get(opts, :port)
    Logger.info("Heimdall TCP server starting on port #{port}")
    {:ok, socket} = :gen_tcp.listen(port, [:binary, packet: 2, active: false, reuseaddr: true])
    {:ok, %{socket: socket}, {:continue, :accept}}
  end

  def handle_continue(:accept, state) do
    accept_loop(state.socket)
    {:noreply, state}
  end

  defp accept_loop(socket) do
    {:ok, client} = :gen_tcp.accept(socket)
    :gen_tcp.controlling_process(client, self())
    spawn(fn -> handle_client(client) end)
    accept_loop(socket)
  end

  defp handle_client(socket) do
    case :gen_tcp.recv(socket, 0) do
      {:ok, data} ->
        case Heimdall.DNS.Server.query(data) do
          {:ok, response} ->
            :gen_tcp.send(socket, response)

          {:error, reason} ->
            Logger.error("Failed to handle DNS query: #{reason}")
        end

        handle_client(socket)

      {:error, _reason} ->
        :gen_tcp.close(socket)
    end
  end
end
