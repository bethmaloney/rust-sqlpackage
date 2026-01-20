CREATE TABLE [dbo].[CheckConstraintTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Age] INT NOT NULL,
    [Status] NVARCHAR(20) NOT NULL,
    CONSTRAINT [CK_CheckConstraintTable_Age] CHECK ([Age] >= 0 AND [Age] <= 150),
    CONSTRAINT [CK_CheckConstraintTable_Status] CHECK ([Status] IN ('Active', 'Inactive', 'Pending'))
);
