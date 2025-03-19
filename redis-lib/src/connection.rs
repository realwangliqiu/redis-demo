use crate::frame::{self, Frame};
use bytes::{Buf, BytesMut};
use std::io::{self, Cursor};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

 
/// `Connection` is to read(receive) and write(Send) `Frame` on the underlying `TcpStream`.
///
/// `read_buf` is filled up until there are enough bytes to create a full frame. Once this happens,
/// the `Connection` creates the frame and returns it to the caller. 
#[derive(Debug)]
pub struct Connection { 
    stream: BufWriter<TcpStream>, 
    // The buffer for reading frames.
    read_buf: BytesMut,
}

const BUF_SIZE: usize = 4 * 1024;

impl Connection { 
    pub fn new(stream: TcpStream) -> Connection {
        Connection { 
            stream: BufWriter::new(stream), 
            read_buf: BytesMut::with_capacity(BUF_SIZE),
        }
    }

    /// Read a single `Frame` value from the underlying stream.
    ///
    /// The function waits until it has retrieved enough data to parse a frame.
    /// Any data remaining in the read buffer after the frame has been parsed is
    /// kept there for the next call to `read_frame`.
    ///
    /// # Returns
    ///
    /// On success, the received frame is returned. If the `TcpStream`
    /// is closed in a way that doesn't break a frame in half, it returns
    /// `None`. Otherwise, an error is returned.
    pub async fn read_frame(&mut self) -> crate::Result<Option<Frame>> {
        loop {
            // Attempt to parse a frame from the buffered data. If enough data
            // has been buffered, the frame is returned.
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            // There is not enough buffered data to read a frame. Attempt to
            // read more data from `TcpStream`. 
            // `0` indicates "end of stream".
            if 0 == self.stream.read_buf(&mut self.read_buf).await? {
                // The remote closed the connection. For this to be a clean
                // shutdown, there should be no data in the read buffer. 
                if self.read_buf.is_empty() {
                    return Ok(None);
                }
                return Err("connection reset by peer".into());
            }
        }
    }

    /// Tries to parse a frame from the buffer. If the buffer contains enough
    /// data, the frame is returned and the data removed from the buffer. If not
    /// enough data has been buffered yet, `Ok(None)` is returned. If the
    /// buffered data does not represent a valid frame, `Err` is returned.
    fn parse_frame(&mut self) -> crate::Result<Option<Frame>> {
        use frame::Error::Incomplete;

        // Cursor implements `Buf` from the `bytes` crate 
        let mut buf = Cursor::new(&self.read_buf[..]);

        // The first step is to check if enough data has been buffered to parse
        // a single frame. This step is usually much faster than doing a full
        // parse of the frame, and allows us to skip allocating data structures
        // to hold the frame data unless we know the full frame has been
        // received.
        match Frame::check(&mut buf) {
            Ok(_) => {
                // The `check` function will have advanced the cursor until the end of the frame.  
                let len = buf.position() as usize;

                // Reset the position to zero before passing the cursor to `Frame::parse`.
                buf.set_position(0); 
                let frame = Frame::parse(&mut buf)?;
                // Discard the parsed data from the read buffer.
                self.read_buf.advance(len);

                Ok(Some(frame))
            }
            // There is not enough data present in the read buffer to parse a single frame. 
            Err(Incomplete) => Ok(None),
            // Returning `Err` from here will result in the connection being closed.
            Err(e) => Err(e.into()),
        }
    }

    /// Write a single `Frame` to the underlying stream.
    pub async fn write_frame(&mut self, frame: &Frame) -> io::Result<()> {
        match frame {
            Frame::Array(val) => {
                // Encode the frame type prefix. For an array, it is `*`.
                self.stream.write_u8(b'*').await?;
                // Encode the length of the array.
                self.write_decimal(val.len() as u64).await?;

                for entry in val {
                    self.write_value(entry).await?;
                }
            }
            // The frame type is a literal. Encode the value directly.
            _ => self.write_value(frame).await?,
        }

        // flush the calls above.
        self.stream.flush().await
    }

    /// Write a frame literal to the stream.
    async fn write_value(&mut self, frame: &Frame) -> io::Result<()> {
        match frame {
            Frame::Simple(val) => {
                self.stream.write_u8(b'+').await?;
                self.stream.write_all(val.as_bytes()).await?;
                self.stream.write_all(b"\r\n").await?;
            }
            Frame::Error(val) => {
                self.stream.write_u8(b'-').await?;
                self.stream.write_all(val.as_bytes()).await?;
                self.stream.write_all(b"\r\n").await?;
            }
            Frame::Integer(val) => {
                self.stream.write_u8(b':').await?;
                self.write_decimal(*val).await?;
            }
            Frame::Null => {
                self.stream.write_all(b"$-1\r\n").await?;
            }
            Frame::Bulk(val) => {
                let len = val.len();

                self.stream.write_u8(b'$').await?;
                self.write_decimal(len as u64).await?;
                self.stream.write_all(val).await?;
                self.stream.write_all(b"\r\n").await?;
            }           
            Frame::Array(_) => unreachable!(),
        }

        Ok(())
    }

    /// Write a decimal frame to the stream
    async fn write_decimal(&mut self, val: u64) -> io::Result<()> {
        use std::io::Write;

        // val should be converted to string before writing it to the stream.
        let mut buf = [0u8; 20];
        let mut buf = Cursor::new(&mut buf[..]);
        write!(&mut buf, "{}", val)?;

        let pos = buf.position() as usize;
        self.stream.write_all(&buf.get_ref()[..pos]).await?;
        self.stream.write_all(b"\r\n").await?;

        Ok(())
    }
}
