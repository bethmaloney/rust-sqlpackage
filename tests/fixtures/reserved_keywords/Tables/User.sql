-- Table with reserved keyword 'User'
CREATE TABLE [dbo].[User] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Key] NVARCHAR(50) NOT NULL,       -- Reserved keyword as column
    [Value] NVARCHAR(500) NULL,        -- Reserved keyword as column
    [Table] NVARCHAR(100) NULL         -- Reserved keyword as column
);
GO
