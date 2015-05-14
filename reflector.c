/* GStreamer
 * Copyright (C) 2015 Sebastian Dröge <sebastian@centricular.com>
 *
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Library General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * Library General Public License for more details.
 *
 * You should have received a copy of the GNU Library General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 51 Franklin St, Fifth Floor,
 * Boston, MA 02110-1301, USA.
 */

#include <glib.h>
#include <gio/gio.h>

static GSocketAddress *event_saddr, *general_saddr;
static GSocket *socket_event, *socket_general;

static gboolean
have_socket_data_cb (GSocket * socket, GIOCondition condition,
    gpointer user_data)
{
  gchar buffer[8192];
  gssize read;
  gssize written;
  GError *err = NULL;
  guint domain, type;
  gboolean forward = FALSE;

  read = g_socket_receive (socket, buffer, sizeof (buffer), NULL, &err);
  if (read == -1)
    g_error ("Failed to read from socket: %s", err->message);

  g_message ("Received %" G_GSSIZE_FORMAT " bytes from %s socket", read,
      (socket == socket_event ? "event" : "general"));

  if (read < 34) {
    g_message ("short read");
    return G_SOURCE_CONTINUE;
  }

  domain = buffer[4];
  type = buffer[0] & 0x0f;
  /* Change domain 0 to domain 1 and the other way around */
  if (domain == 0 && (type == 0x0 || type == 0x8 || type == 0xb || type == 0x9)) {
    gint i;

    buffer[4] = 1;

    /* flip source clock id */
    for (i = 20; i < 28; i++)
      buffer[i] = buffer[i] ^ 0xff;

    /* flip source clock id */
    for (i = 44; i < 52; i++)
      buffer[i] = buffer[i] ^ 0xff;

    forward = TRUE;
  } else if (domain == 1 && (type == 0x1)) {
    gint i;

    buffer[4] = 0;

    /* flip source clock id */
    for (i = 20; i < 28; i++)
      buffer[i] = buffer[i] ^ 0xff;

    forward = TRUE;
  }

  if (forward) {

    written =
        g_socket_send_to (socket,
        (socket == socket_event ? event_saddr : general_saddr), buffer,
        read, NULL, &err);

    if (written != read)
      g_warning ("written %" G_GSIZE_FORMAT " != read %" G_GSIZE_FORMAT,
          written, read);
  }

  return G_SOURCE_CONTINUE;
}

gint
main (gint argc, gchar ** argv)
{
  GMainLoop *loop;
  GError *err = NULL;
  GInetAddress *bind_addr, *mcast_addr;
  GSocketAddress *bind_saddr;
  GSource *socket_event_source, *socket_general_source;

  /* Create sockets */
  socket_event =
      g_socket_new (G_SOCKET_FAMILY_IPV4, G_SOCKET_TYPE_DATAGRAM,
      G_SOCKET_PROTOCOL_UDP, &err);
  if (!socket_event)
    g_error ("Couldn't create event socket: %s", err->message);
  g_socket_set_multicast_loopback (socket_event, FALSE);

  socket_general =
      g_socket_new (G_SOCKET_FAMILY_IPV4, G_SOCKET_TYPE_DATAGRAM,
      G_SOCKET_PROTOCOL_UDP, &err);
  if (!socket_general)
    g_error ("Couldn't create general socket: %s", err->message);
  g_socket_set_multicast_loopback (socket_general, FALSE);

  /* Bind sockets */
  bind_addr = g_inet_address_new_any (G_SOCKET_FAMILY_IPV4);
  bind_saddr = g_inet_socket_address_new (bind_addr, 319);
  if (!g_socket_bind (socket_event, bind_saddr, TRUE, &err))
    g_error ("Couldn't bind event socket: %s", err->message);
  g_object_unref (bind_saddr);
  bind_saddr = g_inet_socket_address_new (bind_addr, 320);
  if (!g_socket_bind (socket_general, bind_saddr, TRUE, &err))
    g_error ("Couldn't bind general socket: %s", err->message);
  g_object_unref (bind_saddr);
  g_object_unref (bind_addr);

  /* Join multicast groups */
  mcast_addr = g_inet_address_new_from_string ("224.0.1.129");

  /* Join multicast group without any interface */
  if (!g_socket_join_multicast_group (socket_event, mcast_addr, FALSE, NULL,
          &err))
    g_error ("Couldn't join multicast group: %s", err->message);
  if (!g_socket_join_multicast_group (socket_general, mcast_addr, FALSE,
          NULL, &err))
    g_error ("Couldn't join multicast group: %s", err->message);

  event_saddr = g_inet_socket_address_new (mcast_addr, 319);
  general_saddr = g_inet_socket_address_new (mcast_addr, 320);

  /* Create socket sources */
  socket_event_source =
      g_socket_create_source (socket_event, G_IO_IN | G_IO_PRI, NULL);
  g_source_set_priority (socket_event_source, G_PRIORITY_HIGH);
  g_source_set_callback (socket_event_source, (GSourceFunc) have_socket_data_cb,
      NULL, NULL);
  g_source_attach (socket_event_source, NULL);
  socket_general_source =
      g_socket_create_source (socket_general, G_IO_IN | G_IO_PRI, NULL);
  g_source_set_priority (socket_general_source, G_PRIORITY_DEFAULT);
  g_source_set_callback (socket_general_source,
      (GSourceFunc) have_socket_data_cb, NULL, NULL);
  g_source_attach (socket_general_source, NULL);

  /* Get running */
  loop = g_main_loop_new (NULL, FALSE);
  g_main_loop_run (loop);
  g_main_loop_unref (loop);

  g_source_destroy (socket_event_source);
  g_source_destroy (socket_general_source);

  /* Leave multicast groups */
  g_socket_leave_multicast_group (socket_event, mcast_addr, FALSE, NULL, NULL);
  g_socket_leave_multicast_group (socket_general, mcast_addr, FALSE, NULL,
      NULL);
  g_object_unref (mcast_addr);
  g_object_unref (event_saddr);
  g_object_unref (general_saddr);

  g_socket_close (socket_event, NULL);
  g_object_unref (socket_event);
  g_socket_close (socket_general, NULL);
  g_object_unref (socket_general);

  return 0;
}
