//!HID consumer control devices

// Consumer descriptor from MS natural keyboard
// Report Descriptor: (length is 86)
// Item(Global): Usage Page, data= [ 0x0c ] 12
//                 Consumer
// Item(Local ): Usage, data= [ 0x01 ] 1
//                 Consumer Control
// Item(Main  ): Collection, data= [ 0x01 ] 1
//                 Application
// Item(Global): Report ID, data= [ 0x01 ] 1
// Item(Global): Usage Page, data= [ 0x0c ] 12
//                 Consumer
// Item(Local ): Usage Minimum, data= [ 0x00 ] 0
//                 Unassigned
// Item(Local ): Usage Maximum, data= [ 0xff 0x03 ] 1023
//                 (null)
// Item(Global): Report Count, data= [ 0x01 ] 1
// Item(Global): Report Size, data= [ 0x10 ] 16
// Item(Global): Logical Minimum, data= [ 0x00 ] 0
// Item(Global): Logical Maximum, data= [ 0xff 0x03 0x00 0x00 ] 1023
// Item(Main  ): Input, data= [ 0x00 ] 0
//                 Data Array Absolute No_Wrap Linear
//                 Preferred_State No_Null_Position Non_Volatile Bitfield
// Item(Global): Usage Page, data= [ 0x07 ] 7
//                 Keyboard
// Item(Local ): Usage Minimum, data= [ 0x00 ] 0
//                 No Event
// Item(Local ): Usage Maximum, data= [ 0xff ] 255
//                 (null)
// Item(Global): Report Size, data= [ 0x08 ] 8
// Item(Global): Logical Maximum, data= [ 0xff 0x00 ] 255
// Item(Main  ): Input, data= [ 0x00 ] 0
//                 Data Array Absolute No_Wrap Linear
//                 Preferred_State No_Null_Position Non_Volatile Bitfield
// Item(Main  ): Input, data= [ 0x01 ] 1
//                 Constant Array Absolute No_Wrap Linear
//                 Preferred_State No_Null_Position Non_Volatile Bitfield
// Item(Global): Usage Page, data= [ 0x00 0xff ] 65280
//                 (null)
// Item(Local ): Usage, data= [ 0x03 0xfe ] 65027
//                 (null)
// Item(Local ): Usage, data= [ 0x04 0xfe ] 65028
//                 (null)
// Item(Global): Report Count, data= [ 0x02 ] 2
// Item(Global): Report Size, data= [ 0x01 ] 1
// Item(Global): Logical Maximum, data= [ 0x01 ] 1
// Item(Main  ): Input, data= [ 0x02 ] 2
//                 Data Variable Absolute No_Wrap Linear
//                 Preferred_State No_Null_Position Non_Volatile Bitfield
// Item(Local ): Usage, data= [ 0x05 0xff ] 65285
//                 (null)
// Item(Global): Report Count, data= [ 0x01 ] 1
// Item(Global): Report Size, data= [ 0x05 ] 5
// Item(Global): Logical Maximum, data= [ 0x1f ] 31
// Item(Main  ): Input, data= [ 0x02 ] 2
//                 Data Variable Absolute No_Wrap Linear
//                 Preferred_State No_Null_Position Non_Volatile Bitfield
// Item(Global): Report Size, data= [ 0x09 ] 9
// Item(Main  ): Input, data= [ 0x01 ] 1
//                 Constant Array Absolute No_Wrap Linear
//                 Preferred_State No_Null_Position Non_Volatile Bitfield
// Item(Local ): Usage, data= [ 0x02 0xff ] 65282
//                 (null)
// Item(Global): Logical Maximum, data= [ 0xff 0x00 ] 255
// Item(Global): Report Size, data= [ 0x08 ] 8
// Item(Main  ): Input, data= [ 0x02 ] 2
//                 Data Variable Absolute No_Wrap Linear
//                 Preferred_State No_Null_Position Non_Volatile Bitfield
// Item(Main  ): End Collection, data=none

//Notes:
//
