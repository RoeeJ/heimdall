defmodule HeimdallWeb.ZoneLive do
  use HeimdallWeb, :live_view
  use HeimdallWeb, :verified_routes
  alias Heimdall.DNS.Manager

  defp supported_record_types, do: [:a, :ns, :cname, :hinfo, :mx, :txt, :aaaa, :loc, :srv]

  def mount(%{"zone_id" => zone_id}, _, socket) do
    zone_id = String.to_integer(zone_id)

    socket = socket |> assign(zone_id: zone_id) |> reset_new_record() |> refresh_zone()
    {:ok, socket}
  end

  defp convert_params(params) do
    Enum.into(params, %{}, fn {k, v} ->
      key = String.to_existing_atom(k)
      value = if key == :type, do: String.to_existing_atom(v), else: v
      {key, value}
    end)
  end

  def render(assigns) do
    ~H"""
    <div>
      <h1>Edit DNS Zone</h1>

      <div>
        <h2>Zone Details</h2>

        <p><strong>ID:</strong> <%= @zone.id %></p>

        <p><strong>Name:</strong> <%= @zone.name %></p>

        <p><strong>Serial:</strong> <%= @zone.serial %></p>
      </div>

      <div>
        <h2>Records</h2>

        <table>
          <thead>
            <tr>
              <th>Name</th>

              <th>Type</th>

              <th>TTL</th>

              <th>Data</th>

              <th>Actions</th>
            </tr>
          </thead>

          <tbody>
            <%= for record <- @zone.records do %>
              <tr>
                <td colspan="5">
                  <form phx-change="update_record" phx-value-id={record.id} phx-submit="save_record">
                    <input
                      type="text"
                      name="record[name]"
                      value={record.name}
                      phx-value-field="name"
                      pattern="^(@|\*|([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])(\.([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]{0,61}[a-zA-Z0-9]))*)?$"
                      required
                    />
                    <select name="record[type]" phx-value-field="type" required>
                      <%= for type <- supported_record_types() do %>
                        <option value={type} selected={type == record.type}>
                          <%= String.upcase(Atom.to_string(type)) %>
                        </option>
                      <% end %>
                    </select>

                    <.ttl_dropdown
                      name="record[ttl]"
                      value={record.ttl}
                      phx-value-field="ttl"
                      field="ttl"
                      required
                    /> <.record_input record={record} />
                    <button phx-value-id={record.id} class="fa-floppy-disk" type="submit" />
                    <button
                      phx-click="delete_record"
                      phx-value-id={record.id}
                      class="fa-trash-can"
                      type="button"
                    />
                  </form>
                </td>
              </tr>
            <% end %>
          </tbody>
        </table>

        <form phx-submit="add_record" phx-change="update_new_record">
          <input
            type="text"
            name="record[name]"
            placeholder="Name"
            value={@new_record.name}
            pattern="^(@|\*|([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])(\.([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]{0,61}[a-zA-Z0-9]))*)?$"
            required
          />
          <select name="record[type]" value={@new_record.type} required>
            <%= for type <- supported_record_types() do %>
              <option value={type} selected={type == @new_record.type}>
                <%= String.upcase(Atom.to_string(type)) %>
              </option>
            <% end %>
          </select>

          <.ttl_dropdown
            name="record[ttl]"
            value={@new_record.ttl}
            phx-value-field="ttl"
            field="ttl"
            required
          /> <.record_input record={@new_record} />
          <button type="submit" class="fa-plus">Add Record</button>
        </form>
      </div>

      <%= if @live_action == :edit do %>
        <button phx-click="apply_changes">Apply Changes</button>
      <% end %>
    </div>
    """
  end

  def handle_event("update_record", %{"id" => id, "record" => record_params}, socket) do
    updated_records =
      Enum.map(socket.assigns.zone.records, fn record ->
        if record.id == String.to_integer(id) do
          Map.merge(record, convert_params(record_params))
        else
          record
        end
      end)

    {:noreply, assign(socket, :zone, %{socket.assigns.zone | records: updated_records})}
  end

  def handle_event("save_record", %{"id" => id}, socket) do
    record =
      Enum.find(socket.assigns.zone.records, fn record -> record.id == String.to_integer(id) end)

    case Manager.update_record(record.id, Map.from_struct(record)) do
      {:ok, _record} ->
        socket =
          socket
          |> assign(:zone, Manager.get_zone(socket.assigns.zone_id))
          |> push_navigate(to: ~p"/zones/#{socket.assigns.zone_id}")

        {:noreply, socket}

      {:error, _changeset} ->
        {:noreply, socket}
    end
  end

  def handle_event("delete_record", %{"id" => id}, socket) do
    case Manager.delete_record(String.to_integer(id)) do
      {:ok, _record} ->
        socket =
          socket
          |> assign(:zone, Manager.get_zone(socket.assigns.zone_id))
          |> push_navigate(to: ~p"/zones/#{socket.assigns.zone_id}")

        {:noreply, socket}

      {:error, _reason} ->
        {:noreply, socket}
    end
  end

  def handle_event("update_new_record", params, socket) do
    updated_record =
      Enum.reduce(params["record"], socket.assigns.new_record, fn {key, value}, acc ->
        case key do
          "name" -> %{acc | name: value}
          "type" -> %{acc | type: String.to_existing_atom(value)}
          "ttl" -> %{acc | ttl: String.to_integer(value)}
          "data" -> %{acc | data: update_record_data(acc.type, value)}
          _ -> acc
        end
      end)

    {:noreply, assign(socket, :new_record, updated_record)}
  end

  def handle_event("add_record", params, socket) do
    updated_record =
      Enum.reduce(params["record"], %{}, fn {key, value}, acc ->
        case key do
          "name" ->
            Map.put(acc, :name, value)

          "type" ->
            Map.put(acc, :type, String.to_existing_atom(value))

          "ttl" ->
            Map.put(acc, :ttl, String.to_integer(value))

          "data" ->
            Map.put(
              acc,
              :data,
              update_record_data(String.to_existing_atom(params["record"]["type"]), value)
            )

          _ ->
            acc
        end
      end)

    params = Map.put(params, "record", updated_record)

    case Manager.add_record(socket.assigns.zone_id, params["record"]) do
      {:ok, _record} ->
        socket = socket |> reset_new_record() |> refresh_zone()
        {:noreply, socket}

      {:error, _changeset} ->
        {:noreply, socket}
    end
  end

  defp update_record_data(type, data) do
    case type do
      :a -> Map.take(data, ["ip"])
      :cname -> Map.take(data, ["host"])
      :ns -> Map.take(data, ["host"])
      :txt -> Map.take(data, ["text"])
      :mx -> Map.take(data, ["preference", "host"])
      _ -> data
    end
  end

  defp ttl_dropdown(assigns) do
    ~H"""
    <select name={@name} value={@value} phx-value-field={@field} required>
      <option value="60" selected={60 == @value}>1 minute</option>

      <option value="300" selected={300 == @value}>5 minutes</option>

      <option value="600" selected={600 == @value}>10 minutes</option>

      <option value="900" selected={900 == @value}>15 minutes</option>

      <option value="1800" selected={1800 == @value}>30 minutes</option>

      <option value="3600" selected={3600 == @value}>1 hour</option>

      <option value="7200" selected={7200 == @value}>2 hours</option>

      <option value="14400" selected={14400 == @value}>4 hours</option>

      <option value="28800" selected={28800 == @value}>8 hours</option>

      <option value="57600" selected={57600 == @value}>16 hours</option>

      <option value="86400" selected={86400 == @value}>1 day</option>
    </select>
    """
  end

  @spec record_input(%{record: Heimdall.Schema.Record.t()}) :: Phoenix.LiveView.Rendered.t()
  defp record_input(assigns) do
    ~H"""
    <%= case @record.type do %>
      <% :a -> %>
        <.ip_input
          value={@record.data["ip"]}
          name="record[data][ip]"
          field="data[ip]"
          pattern="^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$"
        />
      <% :aaaa -> %>
        <.ip_input
          value={@record.data["ip"]}
          name="record[data][ip]"
          field="data[ip]"
          pattern="(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))"
        />
      <% :ns -> %>
        <.host_input value={@record.data["host"]} name="record[data][host]" field="data[host]" />
      <% :cname -> %>
        <.host_input value={@record.data["host"]} name="record[data][host]" field="data[host]" />
      <% :txt -> %>
        <.text_input
          value={@record.data["text"]}
          name="record[data][text]"
          field="data[text]"
          maxlength="255"
        />
      <% :mx -> %>
        <.mx_input preference={@record.data["preference"]} host={@record.data["host"]} />
      <% _ -> %>
        <div>Unsupported record type: <%= @record.type %></div>
    <% end %>
    """
  end

  defp ip_input(assigns) do
    ~H"""
    <input
      type="text"
      value={@value}
      name={@name}
      phx-value-field={@field}
      pattern={@pattern}
      required
    />
    """
  end

  defp host_input(assigns) do
    ~H"""
    <input
      type="text"
      value={@value}
      name={@name}
      phx-value-field={@field}
      pattern="^([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])(\.([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]{0,61}[a-zA-Z0-9]))*$"
      required
    />
    """
  end

  defp text_input(assigns) do
    ~H"""
    <input
      type="text"
      value={@value}
      name={@name}
      phx-value-field={@field}
      maxlength={@maxlength}
      required
    />
    """
  end

  defp mx_input(assigns) do
    ~H"""
    <input
      type="number"
      value={@preference}
      name="record[data][preference]"
      phx-value-field="data[preference]"
      min="0"
      max="65535"
      required
    />
    <input
      type="text"
      value={@host}
      name="record[data][host]"
      phx-value-field="data[host]"
      pattern="^([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])(\.([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9\-]{0,61}[a-zA-Z0-9]))*$"
      required
    />
    """
  end

  @spec refresh_zone(Phoenix.LiveView.Socket.t()) :: Phoenix.LiveView.Socket.t()
  defp refresh_zone(socket) do
    case Manager.get_zone(socket.assigns.zone_id) do
      {:ok, zone} ->
        socket
        |> assign(:zone, zone)

      {:error, :zone_not_found} ->
        socket
    end
  end

  @spec reset_new_record(Phoenix.LiveView.Socket.t()) :: Phoenix.LiveView.Socket.t()
  defp reset_new_record(socket) do
    socket
    |> assign(:new_record, %Heimdall.Schema.Record{ttl: 3600, type: :a})
  end
end
