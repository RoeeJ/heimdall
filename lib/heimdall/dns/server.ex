defmodule Heimdall.DNS.Server do
  @moduledoc """
  GenServer for handling DNS requests.
  """
  use GenServer
  require Logger
  alias Heimdall.DNS.{Encoder, Decoder, Model, Resolver}

  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts)
  end

  def init(opts) do
    port = Keyword.get(opts, :port)
    Logger.info("Heimdall DNS starting on port #{port}")
    {:ok, socket} = :gen_udp.open(port, [:binary, active: true])
    {:ok, %{socket: socket}}
  end

  def handle_info({:udp, socket, address, port, data}, state) do
    Task.start(fn ->
      handle_dns_query(socket, address, port, data)
    end)

    {:noreply, state}
  end

  def handle_info(_msg, state) do
    {:noreply, state}
  end

  defp handle_dns_query(socket, address, port, data) do
    case Decoder.packet(data) do
      %Model.Packet{} = packet ->
        response = process_packet(packet)
        send_response(socket, address, port, response)

      _ ->
        Logger.error("Failed to decode DNS packet")
    end
  end

  defp process_packet(%Model.Packet{} = packet) do
    answers = resolve_queries(packet.questions)
    additional = process_additional(packet.additional)

    %Model.Packet{
      packet
      | qr: :response,
        recurs_available: true,
        answers: answers,
        ancount: length(answers),
        additional: additional,
        arcount: length(additional)
    }
  end

  defp resolve_queries(questions) do
    Enum.flat_map(questions, fn %Model.Question{} = q ->
      case Resolver.query(q.qname, q.qtype) do
        {:ok, resources} ->
          resources

        {:error, reason} ->
          Logger.error("Got err: #{reason} when querying #{q.qname} for #{q.qtype} records")
          []
      end
    end)
  end

  defp process_additional(additional) do
    Enum.map(additional, fn
      %Model.ResourceRecord{qtype: :opt, rdata: %Model.EDNS{} = edns} = rr ->
        %Model.ResourceRecord{rr | rdata: process_edns(edns)}

      other ->
        other
    end)
  end

  defp process_edns(%Model.EDNS{} = edns) do
    %Model.EDNS{edns | data: process_edns_data(edns.data)}
  end

  defp process_edns_data(%Model.EDNS.Cookie{} = cookie) do
    %Model.EDNS.Cookie{cookie | server_cookie: generate_server_cookie(), opt_length: 16}
  end

  defp process_edns_data(other), do: other

  defp generate_server_cookie do
    <<Heimdall.Constants.server_cookie()::64>>
  end

  defp send_response(socket, address, port, response) do
    encoded_response = Encoder.packet(response)

    case :gen_udp.send(socket, address, port, encoded_response) do
      :ok ->
        :ok

      {:error, reason} ->
        Logger.error("Failed to send DNS response: #{inspect(reason)}")
    end
  end
end
