-- Table with reserved keyword 'Order'
CREATE TABLE [dbo].[Order] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Select] NVARCHAR(100) NOT NULL,  -- Reserved keyword as column
    [From] DATETIME NOT NULL,          -- Reserved keyword as column
    [Where] NVARCHAR(500) NULL         -- Reserved keyword as column
);
GO
