defmodule Heimdall.DNS.Server do
  use GenServer
  require Logger
  alias Heimdall.DNS.{Encoder, Decoder, Model, Cache}

  def start_link(port \\ 1053) do
    GenServer.start_link(__MODULE__, port)
  end

  def init(port) do
    :gen_udp.open(port, [:binary, active: true])
  end

  def handle_info({:udp, socket, address, port, data}, state) do
    response =
      %Model.Packet{Decoder.packet(data) | qr: :response, ra: 1}
      |> handle_packet()

    case :gen_udp.send(socket, address, port, response) do
      :ok ->
        Logger.debug("Sent reply to #{Tuple.to_list(address) |> Enum.join(".")}")

      {:error, err} ->
        Logger.error("Err while sending: #{err}")
    end

    {:noreply, socket}
  end

  @spec handle_packet(packet :: Model.Packet.t()) :: bitstring()
  defp handle_packet(%Model.Packet{} = packet) do
    answers =
      for %Model.Question{} = q <- packet.questions do
        case Heimdall.DNS.Resolver.resolve(q.qname, q.qtype) do
          {:ok, [res]} ->
            Logger.info("Got response: #{Tuple.to_list(res) |> Enum.join(".")}")

          {:error, err} ->
            Logger.error("Got error: #{Atom.to_string(err)}")
        end
      end

    additional =
      for %Model.Additional{} = ar <- packet.additional do
        rdata =
          case ar.rdata do
            %Model.EDNS{} = edns ->
              edns_data =
                case edns.data do
                  %Model.EDNS.Cookie{} = cookie ->
                    %Model.EDNS.Cookie{cookie | server_cookie: <<0::64>>, opt_length: 16}

                  _ ->
                    edns.data
                end

              %Model.EDNS{edns | data: edns_data}
          end

        %Model.Additional{ar | rdata: rdata}
      end

    %Model.Packet{packet | qr: :response, additional: additional}
    |> Encoder.packet()
  end
end
