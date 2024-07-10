defmodule Heimdall.DNS.Server do
  use GenServer
  alias Heimdall.DNS.Model
  alias Heimdall.DNS.Cache

  def start_link(port \\ 1053) do
    GenServer.start_link(__MODULE__, port)
  end

  def init(port) do
    :gen_udp.open(port, [:binary, active: true])
  end

  def handle_info({:udp, _socket, _address, _port, data}, socket) do
    handle_packet(data, socket)
  end

  defp handle_packet(data, socket) do
    packet = Heimdall.DNS.Parser.parse(data)

    responses =
      for %Model.Question{} = q <- packet.questions do
        Cache.query(q.qname, q.qtype)
      end

    IO.inspect(responses)
    {:noreply, socket}
  end
end
